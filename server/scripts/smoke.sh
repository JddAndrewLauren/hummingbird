#!/usr/bin/env bash
# The wire-level half of #114's acceptance: boots `wrangler dev` against the
# real SQLite-backed Durable Object and asserts token minting, scope
# enforcement, the create / replay / CAS behaviours, alert ingest, the
# sweep/delta byte agreement, and revocation over HTTP. Statement-level
# criteria (statement counts, no-bump properties) are proven by the native
# fixture suite, not here — the wire can only show status and bodies.
#
# Run from server/: ./scripts/smoke.sh
set -euo pipefail

# jq backs every assertion below — fail before any expensive work. curl gets
# the same guard: without it the port check silently passes and the readiness
# loop burns its whole budget before failing.
command -v jq >/dev/null || { echo "FAIL: jq is required" >&2; exit 1; }
command -v curl >/dev/null || { echo "FAIL: curl is required" >&2; exit 1; }

PORT="${SMOKE_PORT:-8787}"
BASE="http://127.0.0.1:${PORT}"
ADMIN_SECRET="smoke-admin-secret"
cd "$(dirname "$0")/../worker"

# A previous run's surviving workerd would answer the poll with a server
# whose state this script is about to delete — fail fast instead.
if curl -s -o /dev/null --max-time 2 "$BASE/"; then
  echo "FAIL: something already listens on port $PORT — kill it first" >&2
  exit 1
fi

# A stale local dev DB would break replay/version assertions.
rm -rf .wrangler/state

# Pre-warm wrangler's [build] command (same invocation as wrangler.toml):
# on a cold runner `cargo install worker-build` alone takes minutes, which
# would eat the whole readiness budget below. Warm, wrangler's own re-run
# of it is a fast no-op.
cargo install -q worker-build && worker-build --release

export CI=true WRANGLER_SEND_METRICS=false
# The trap must kill the whole tree — killing only the npx wrapper orphans
# the wrangler node process and its workerd, which then squats the port with
# a state dir the next run deletes. With setsid (Linux) the tree gets its own
# process group and one group kill suffices; without it (macOS) fall back to
# killing the wrapper's descendants first, then the wrapper.
if command -v setsid >/dev/null; then
  setsid npx --yes wrangler@4 dev --port "$PORT" --var "ADMIN_SECRET:$ADMIN_SECRET" \
    >/tmp/wrangler-smoke.log 2>&1 &
  WRANGLER_PID=$!
  trap 'kill -- "-$WRANGLER_PID" 2>/dev/null || true' EXIT
else
  npx --yes wrangler@4 dev --port "$PORT" --var "ADMIN_SECRET:$ADMIN_SECRET" \
    >/tmp/wrangler-smoke.log 2>&1 &
  WRANGLER_PID=$!
  trap 'pkill -P "$WRANGLER_PID" 2>/dev/null || true; kill "$WRANGLER_PID" 2>/dev/null || true' EXIT
fi

# The build is warm; this budget covers the workerd download and boot. The
# probe doubles as the first auth assertion: an unauthenticated read must be
# a 401 — a 200 here would mean the server is up with auth off.
for _ in $(seq 1 60); do
  code=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/api/changes?since=0" || true)
  [ "$code" = "401" ] && break
  [ "$code" = "200" ] && { echo "FAIL: unauthenticated read got 200 — auth is off" >&2; exit 1; }
  sleep 5
done
if [ "${code:-}" != "401" ]; then
  echo "FAIL: wrangler dev never became ready; last 40 log lines:" >&2
  tail -40 /tmp/wrangler-smoke.log >&2
  exit 1
fi

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# One request; asserts the status and leaves the body in $BODY. The optional
# 5th argument is a bearer token (the admin lane passes the admin secret).
request() {
  local expected_status=$1 method=$2 path=$3 data=${4:-} token=${5:-}
  local args=(-s -w '\n%{http_code}' -X "$method" "$BASE$path")
  [ -n "$data" ] && args+=(-d "$data")
  [ -n "$token" ] && args+=(-H "Authorization: Bearer $token")
  local raw
  raw=$(curl "${args[@]}")
  # Split on the last newline (BSD head rejects `head -n -1`).
  BODY=${raw%$'\n'*}
  local status
  status=${raw##*$'\n'}
  [ "$status" = "$expected_status" ] ||
    fail "$method $path -> $status (wanted $expected_status): $BODY"
}

# ------------------------------------------------------------------ auth

# The wrong admin secret is a clean, empty-bodied 401.
request 401 POST /api/admin/tokens '{"id":"t-x","name":"x","scope":"device"}' "not-the-secret"
[ -z "$BODY" ] || fail "401 leaked a body: $BODY"

# Mint one token per scope; the plaintext appears exactly once.
request 201 POST /api/admin/tokens '{"id":"t-device","name":"smoke device","scope":"device"}' "$ADMIN_SECRET"
DEVICE=$(jq -r '.token' <<<"$BODY")
case "$DEVICE" in hb_*) ;; *) fail "device token shape: $BODY";; esac
request 201 POST /api/admin/tokens '{"id":"t-sweeper","name":"smoke sweeper","scope":"sweeper"}' "$ADMIN_SECRET"
SWEEPER=$(jq -r '.token' <<<"$BODY")
# An ingest token is bound to exactly one source (#145); every alert this
# script raises below uses source "healthchecks/v1", so the smoke ingest
# token is minted bound to it.
request 201 POST /api/admin/tokens '{"id":"t-ingest","name":"smoke ingest","scope":"ingest","source":"healthchecks/v1"}' "$ADMIN_SECRET"
INGEST=$(jq -r '.token' <<<"$BODY")

# Minting an ingest token without a source, or a non-ingest token with one,
# is a 400 (#145).
request 400 POST /api/admin/tokens '{"id":"t-ingest-nosrc","name":"x","scope":"ingest"}' "$ADMIN_SECRET"
request 400 POST /api/admin/tokens '{"id":"t-device-src","name":"x","scope":"device","source":"healthchecks/v1"}' "$ADMIN_SECRET"

# The bound ingest token cannot post an alert for a different source: 403,
# empty body.
request 403 POST /api/alerts '{"source":"home-assistant/v1","source_key":"k","title":"t"}' "$INGEST"
[ -z "$BODY" ] || fail "403 leaked a body: $BODY"

# A replayed mint returns metadata without the plaintext.
request 200 POST /api/admin/tokens '{"id":"t-device","name":"smoke device","scope":"device"}' "$ADMIN_SECRET"
[ "$(jq -r 'has("token")' <<<"$BODY")" = "false" ] || fail "mint replay leaked a token: $BODY"

# An unauthenticated write is an empty-bodied 401 before any routing.
request 401 POST /api/items '{"id":"smoke-1","title":"hello"}'
[ -z "$BODY" ] || fail "401 leaked a body: $BODY"

# ------------------------------------------------- the S0 flow, authed

# Create -> 201, server-stamped version.
request 201 POST /api/items '{"id":"smoke-1","title":"hello"}' "$DEVICE"
V1=$(jq -r '.version' <<<"$BODY")
[ "$(jq -r '.seq' <<<"$BODY")" = "1" ] || fail "create did not mint seq 1: $BODY"

# Replay of the identical create -> 200, same row, no duplicate.
request 200 POST /api/items '{"id":"smoke-1","title":"hello"}' "$DEVICE"
[ "$(jq -r '.version' <<<"$BODY")" = "$V1" ] || fail "replay bumped version: $BODY"
request 200 GET "/api/changes?since=0" '' "$DEVICE"
[ "$(jq -r '.items | length' <<<"$BODY")" = "1" ] || fail "replay duplicated the row: $BODY"

# Stale expected_version -> 409 carrying the current entity.
request 409 PATCH /api/items/smoke-1 '{"expected_version":999999,"title":"x"}' "$DEVICE"
[ "$(jq -r '.error' <<<"$BODY")" = "version_conflict" ] || fail "409 body shape: $BODY"
[ "$(jq -r '.current.version' <<<"$BODY")" = "$V1" ] || fail "409 lacks current entity: $BODY"
[ "$(jq -r '.current.title' <<<"$BODY")" = "hello" ] || fail "409 entity was modified: $BODY"

# Fresh expected_version -> applies and bumps.
request 200 PATCH /api/items/smoke-1 "{\"expected_version\":$V1,\"title\":\"renamed\"}" "$DEVICE"
V2=$(jq -r '.version' <<<"$BODY")
[ "$V2" -gt "$V1" ] || fail "fresh patch did not bump version: $BODY"
[ "$(jq -r '.title' <<<"$BODY")" = "renamed" ] || fail "fresh patch did not apply: $BODY"

# Version-gated read: current cursor -> empty; older cursor -> the change.
request 200 GET "/api/changes?since=$V2" '' "$DEVICE"
[ "$(jq -r '.items | length' <<<"$BODY")" = "0" ] || fail "since=current not empty: $BODY"
[ "$(jq -r '.version' <<<"$BODY")" = "$V2" ] || fail "since=current wrong version: $BODY"
request 200 GET "/api/changes?since=$V1" '' "$DEVICE"
[ "$(jq -r '.items | length' <<<"$BODY")" = "1" ] || fail "since=V1 wrong count: $BODY"
[ "$(jq -r '.items[0].title' <<<"$BODY")" = "renamed" ] || fail "since=V1 wrong row: $BODY"

# ------------------------------------------------------------- scopes

# Device cannot ingest; ingest cannot touch items or read; sweeper creates
# items and nothing else. Every rejection is an empty-bodied 403.
request 403 POST /api/alerts '{"source":"healthchecks/v1","source_key":"k","title":"t"}' "$DEVICE"
[ -z "$BODY" ] || fail "403 leaked a body: $BODY"
request 403 POST /api/items '{"id":"x","title":"t"}' "$INGEST"
request 403 GET "/api/changes?since=0" '' "$INGEST"
request 201 POST /api/items '{"id":"swept-1","title":"from the funnel"}' "$SWEEPER"
request 403 GET "/api/changes?since=0" '' "$SWEEPER"

# ------------------------------------------------------------- alerts

# First raise -> 201; identical re-raise -> 200 with the same version.
request 201 POST /api/alerts '{"source":"healthchecks/v1","source_key":"k","title":"sweeper down","severity":"high"}' "$INGEST"
AV=$(jq -r '.version' <<<"$BODY")
ALERT_ID=$(jq -r '.id' <<<"$BODY")
request 200 POST /api/alerts '{"source":"healthchecks/v1","source_key":"k","title":"sweeper down","severity":"high"}' "$INGEST"
[ "$(jq -r '.version' <<<"$BODY")" = "$AV" ] || fail "identical re-raise bumped: $BODY"
request 200 GET "/api/changes?since=0" '' "$DEVICE"
[ "$(jq -r '.alerts | length' <<<"$BODY")" = "1" ] || fail "alert count in delta: $BODY"

# The device dismisses it — its one alert write.
request 200 PATCH "/api/alerts/$ALERT_ID" "{\"expected_version\":$AV,\"dismissed_at\":1}" "$DEVICE"
[ "$(jq -r '.dismissed_at' <<<"$BODY")" = "1" ] || fail "dismiss did not apply: $BODY"

# ------------------------------------------------------ sweep = delta

SWEEP=$(curl -s -H "Authorization: Bearer $DEVICE" "$BASE/api/sweep")
DELTA=$(curl -s -H "Authorization: Bearer $DEVICE" "$BASE/api/changes?since=0")
[ "$SWEEP" = "$DELTA" ] || fail "sweep and delta-from-zero differ:
sweep: $SWEEP
delta: $DELTA"
[ "$(jq -r 'has("tokens")' <<<"$SWEEP")" = "false" ] || fail "tokens leaked into the sweep: $SWEEP"

# --------------------------------------------------------- revocation

request 200 DELETE /api/admin/tokens/t-device '' "$ADMIN_SECRET"
request 401 GET "/api/changes?since=0" '' "$DEVICE"
[ -z "$BODY" ] || fail "revoked 401 leaked a body: $BODY"

echo "smoke: all assertions passed"
