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
# An ingest token is bound to exactly one source (#145); every alert AND
# every snapshot this script raises below uses source "city-waste/v2", so
# the smoke ingest token is minted bound to it. It has to be a `Writes::Both`
# source (ADR-0014's 2026-08-11 amendment, #254) precisely because this one
# token exercises both `POST /api/alerts` and `POST /api/snapshots` below —
# an alerts-only source like "healthchecks/v1" would now 400 on the
# snapshot half with "not declared for snapshots".
request 201 POST /api/admin/tokens '{"id":"t-ingest","name":"smoke ingest","scope":"ingest","source":"city-waste/v2"}' "$ADMIN_SECRET"
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
request 403 POST /api/alerts '{"source":"city-waste/v2","source_key":"k","title":"t"}' "$DEVICE"
[ -z "$BODY" ] || fail "403 leaked a body: $BODY"
request 403 POST /api/items '{"id":"x","title":"t"}' "$INGEST"
request 403 GET "/api/changes?since=0" '' "$INGEST"
request 201 POST /api/items '{"id":"swept-1","title":"from the funnel"}' "$SWEEPER"
request 403 GET "/api/changes?since=0" '' "$SWEEPER"

# ------------------------------------------------------------- alerts

# First raise -> 201; identical re-raise -> 200 with the same version.
request 201 POST /api/alerts '{"source":"city-waste/v2","source_key":"k","title":"sweeper down","severity":"high"}' "$INGEST"
AV=$(jq -r '.version' <<<"$BODY")
ALERT_ID=$(jq -r '.id' <<<"$BODY")
request 200 POST /api/alerts '{"source":"city-waste/v2","source_key":"k","title":"sweeper down","severity":"high"}' "$INGEST"
[ "$(jq -r '.version' <<<"$BODY")" = "$AV" ] || fail "identical re-raise bumped: $BODY"
request 200 GET "/api/changes?since=0" '' "$DEVICE"
[ "$(jq -r '.alerts | length' <<<"$BODY")" = "1" ] || fail "alert count in delta: $BODY"

# The device dismisses it — its one alert write.
request 200 PATCH "/api/alerts/$ALERT_ID" "{\"expected_version\":$AV,\"dismissed_at\":1}" "$DEVICE"
[ "$(jq -r '.dismissed_at' <<<"$BODY")" = "1" ] || fail "dismiss did not apply: $BODY"

# ------------------------------------------- webhook ingest -> deliver

# #255 made `POST /api/alerts` call the same `hummingbird_authority::deliver`
# that #138's sweep tick uses. Until here this script seeds no rule at all, so
# `evaluate_and_deliver` returns before `deliver` is ever reached and the whole
# hook could be deleted with every assertion above still green.
#
# **Nothing about a logged delivery is readable back through the API.**
# `deliveries` carries no `version` column, so it is deliberately not a
# sweep/delta lane (`changes.rs`'s own note, alongside `push_targets`), there
# is no route that reads it, and the handler's `ApiResponse::deliveries` is
# drained by the worker shim (`std::mem::take`) before the body is serialized
# — the alert row a delivery was logged for is byte-identical to one it was
# not. So the wire-visible proof is the lane **failing closed**: `wrangler dev`
# sets no `FCM_SERVICE_ACCOUNT` secret, and `worker/src/lib.rs` answers each
# `DeliveryOutcome::Logged` it is handed with exactly one `console_error!`
# carrying the word "unsendable" — and answers a `Suppressed` outcome with
# nothing at all. That word is therefore a faithful one-per-delivery-row
# counter for this run, and asserting on it is asserting that the claim row
# was committed, not that anything was sent (nothing is, and per CLAUDE.md
# nothing may be retried).
UNSENDABLE_LOG=/tmp/wrangler-smoke.log
# Counted over the whole log with newlines collapsed, and on a single
# unhyphenated word, so wrangler's own line wrapping cannot split a match.
# `grep` exits 1 on no match, which under `pipefail` would abort the script
# on the legitimate zero — hence the `|| true`.
unsendable_count() {
  tr '\n' ' ' <"$UNSENDABLE_LOG" | { grep -o 'unsendable' || true; } | wc -l | tr -d ' '
}
[ "$(unsendable_count)" = "0" ] || fail "a delivery was logged before any rule existed"

# An enabled rule naming no `event_kind` ("any kind") with no conditions
# matches every event, including the `alert_raised` one the ingest hook
# synthesises; one live push target is what keeps `deliver` off its
# `NoTargets` suppression, which logs nothing by design.
request 201 POST /api/rules \
  '{"id":"r-smoke","name":"ring on any alert","conditions":[],"severity":"high","tier":"normal"}' "$DEVICE"
request 201 POST /api/push_targets \
  '{"id":"pt-smoke","name":"smoke device","platform":"android","fcm_token":"smoke-fcm-token"}' "$DEVICE"

PROBE='{"source":"city-waste/v2","source_key":"deliver-probe","title":"collection moved","severity":"high"}'
request 201 POST /api/alerts "$PROBE" "$INGEST"
PROBE_AV=$(jq -r '.version' <<<"$BODY")
# The byte-identical replay: `upsert` is a no-op, `deliver` is still called
# unconditionally, and its `UNIQUE(alert_id, rule_id, generation, severity)`
# dedupe is what must absorb it — same `raised_at` (kept, not restamped) and
# same severity, so the same generation.
request 200 POST /api/alerts "$PROBE" "$INGEST"
[ "$(jq -r '.version' <<<"$BODY")" = "$PROBE_AV" ] || fail "delivery probe replay bumped: $BODY"

# A second, distinct occurrence is the barrier that makes the negative above
# assertable rather than a timed guess: the sends run in a `wait_until`
# future, so "no second line yet" could just mean "not flushed yet". This
# raise is issued only after the replay has already been answered — and the
# replay's suppression was decided synchronously, inside its own request —
# so once the barrier's line is visible, a duplicate for the first probe
# would have been too.
request 201 POST /api/alerts \
  '{"source":"city-waste/v2","source_key":"deliver-probe-2","title":"second occurrence","severity":"high"}' "$INGEST"

for _ in $(seq 1 30); do
  UNSENDABLE=$(unsendable_count)
  [ "$UNSENDABLE" -ge 2 ] && break
  sleep 1
done
[ "${UNSENDABLE:-0}" -ge 2 ] ||
  fail "POST /api/alerts logged no delivery (saw $UNSENDABLE); the #255 hook did not run. Last 40 log lines:
$(tail -40 "$UNSENDABLE_LOG")"
[ "$UNSENDABLE" = "2" ] ||
  fail "the replayed raise was delivered a second time ($UNSENDABLE deliveries, wanted 2) — the transitions-only dedupe is gone"

# ---------------------------------------------------------- snapshots

# The server-polled lane (#120), through the real DO rather than rusqlite:
# first write 201, byte-identical replay no-op, a fresh stamp on an unchanged
# payload still writing. The ingest token above is bound to "city-waste/v2",
# which is also the source used here — and, since #254, that binding must
# actually be declared `Writes::Both` in the registry for this to work at
# all, not merely a token-source match.
SNAP='{"source":"city-waste/v2","key":"collection","fetched_at":1000,"payload":{"schema":"city-waste/v2","polled_every_ms":86400000,"body":{"n":1}}}'
request 201 POST /api/snapshots "$SNAP" "$INGEST"
SV=$(jq -r '.version' <<<"$BODY")
request 200 POST /api/snapshots "$SNAP" "$INGEST"
[ "$(jq -r '.version' <<<"$BODY")" = "$SV" ] || fail "byte-identical snapshot replay bumped: $BODY"

# `fetched_at` is part of the value: an unchanged payload polled again still
# moves the stamp, or a healthy poller reads as a dead one.
FRESH=${SNAP/\"fetched_at\":1000/\"fetched_at\":87400000}
request 200 POST /api/snapshots "$FRESH" "$INGEST"
[ "$(jq -r '.fetched_at' <<<"$BODY")" = "87400000" ] || fail "fresh poll did not move fetched_at: $BODY"
[ "$(jq -r '.version' <<<"$BODY")" != "$SV" ] || fail "fresh poll did not reach the delta: $BODY"

# A broken envelope is refused at the write, naming what was wrong; a device
# may not write here at all.
request 400 POST /api/snapshots '{"source":"city-waste/v2","key":"k","fetched_at":1,"payload":{"body":{}}}' "$INGEST"
request 403 POST /api/snapshots "$SNAP" "$DEVICE"
[ -z "$BODY" ] || fail "snapshot 403 leaked a body: $BODY"

# ADR-0014's 2026-08-11 amendment (#254): a correctly-bound token whose
# source the registry does not declare for `context_snapshots` is a 400
# naming the gap, not a write. "healthchecks/v1" is a real, live registry
# entry that is `Writes::Alerts` and nothing else — minting a token bound to
# it succeeds (mint checks enrollment + retirement only, not per-table
# declaration), but the snapshot write itself is refused.
request 201 POST /api/admin/tokens '{"id":"t-ingest-hc","name":"smoke ingest hc","scope":"ingest","source":"healthchecks/v1"}' "$ADMIN_SECRET"
INGEST_HC=$(jq -r '.token' <<<"$BODY")
request 400 POST /api/snapshots '{"source":"healthchecks/v1","key":"k","fetched_at":1,"payload":{"schema":"healthchecks/v1","body":{}}}' "$INGEST_HC"
[ "$(jq -r '.message' <<<"$BODY")" = '`healthchecks/v1` is not declared for snapshots' ] || fail "not-declared-for-snapshots message: $BODY"

# GET /api/snapshots (#135-137): the cursor read-back an evaluated-stream
# poller needs. Source-bound for an ingest token exactly like the write
# side; a device token reads any source.
request 200 GET "/api/snapshots?source=city-waste/v2&key=collection" '' "$INGEST"
[ "$(jq -r '.source' <<<"$BODY")" = "city-waste/v2" ] || fail "snapshot read-back: $BODY"
request 200 GET "/api/snapshots?source=city-waste/v2&key=collection" '' "$DEVICE"
request 403 GET "/api/snapshots?source=home-assistant/v1&key=x" '' "$INGEST"
[ -z "$BODY" ] || fail "snapshot read 403 leaked a body: $BODY"
request 404 GET "/api/snapshots?source=city-waste/v2&key=no-such-key" '' "$INGEST"

# GET /api/rules (#135-137): every non-device scope reaches three routes
# now, not one — this is the poller's own read of the live rule set it
# evaluates in memory. A device token reads every rule; an ingest token's
# read is filtered to the rules its bound source's event_kind may see
# (`rules::event_kinds_readable_by`) plus every any-kind rule. The smoke
# ingest token above is bound to "city-waste/v2", which
# `event_kinds_readable_by` has no mapping for, so it reads only rules with
# no `event_kind` at all — this asserts the response shape and the
# device/ingest/sweeper matrix, not the per-kind filter itself (covered by
# the native fixture suite).
request 200 GET /api/rules '' "$DEVICE"
request 200 GET /api/rules '' "$INGEST"
request 403 GET /api/rules '' "$SWEEPER"
[ -z "$BODY" ] || fail "rules 403 leaked a body: $BODY"

# The third of those reads (settings, alongside snapshots and rules above),
# and settings' unset case.
request 201 PUT /api/settings/city-waste-page '{"expected_version":0,"value":"https://city.example/x"}' "$DEVICE"
request 200 GET /api/settings/city-waste-page '' "$INGEST"
[ "$(jq -r '.value' <<<"$BODY")" = '"https://city.example/x"' ] || fail "settings read: $BODY"
request 404 GET /api/settings/trips-calendar '' "$INGEST"

# ------------------------------------------------ the skill-runner proxy

# The only wire coverage `server/worker`'s proxy (#273, ADR-0018) gets — it
# has no test harness, so its two decidable properties are asserted here and
# everything else lives in the natively-tested `skills` module.
#
# 1. The verdict runs BEFORE the secrets are read, so an unauthenticated tap
#    is an empty-bodied 401 that discloses nothing about whether the lane is
#    provisioned.
# 2. Unset secrets fail closed as a 503, never as a 401 — a 401 would make
#    the client re-prompt a device token that is perfectly fine. `wrangler
#    dev` sets neither `RUNNER_BASE_URL` nor `RUNNER_BEARER_TOKEN`, so this
#    is exactly the state under test.
RUN_BODY='{"skill":"microtask","args":{"ref":"smoke-1"}}'
request 401 POST /api/skills/run "$RUN_BODY"
[ -z "$BODY" ] || fail "skills/run 401 leaked a body: $BODY"
request 403 POST /api/skills/run "$RUN_BODY" "$INGEST"
[ -z "$BODY" ] || fail "skills/run 403 leaked a body: $BODY"
request 503 POST /api/skills/run "$RUN_BODY" "$DEVICE"
[ "$(jq -r '.ok' <<<"$BODY")" = "false" ] || fail "skills/run 503 envelope shape: $BODY"
[ "$(jq -r '.error' <<<"$BODY")" = "The cloud runner is not configured on this server." ] ||
  fail "skills/run unconfigured prose: $BODY"
# A known path with the wrong method is a 405, not a 404.
request 405 GET /api/skills/run '' "$DEVICE"

# --------------------------------------------------- the calendar-token mint

# `POST /api/google/calendar_token` (#577/#582): the authority mints a
# Google `calendar.readonly` access token for an authenticated device. This
# is the fail-closed proof — `wrangler dev` sets none of
# GOOGLE_CALENDAR_CLIENT_ID/_SECRET/_REFRESH_TOKEN, so the route answers a
# 503, never a 401 (which would make the client re-prompt a device token
# that is perfectly fine), and the verdict runs before the secrets are read.
request 401 POST /api/google/calendar_token
[ -z "$BODY" ] || fail "calendar_token 401 leaked a body: $BODY"
request 403 POST /api/google/calendar_token '' "$INGEST"
[ -z "$BODY" ] || fail "calendar_token 403 leaked a body: $BODY"
request 503 POST /api/google/calendar_token '' "$DEVICE"
[ "$(jq -r '.error' <<<"$BODY")" = "calendar_unconfigured" ] ||
  fail "calendar_token unconfigured code: $BODY"
[ "$(jq -r '.message' <<<"$BODY")" = \
  "The Google calendar credential is not configured on this server." ] ||
  fail "calendar_token unconfigured prose: $BODY"
# A known path with the wrong method is a 405, not a 404.
request 405 GET /api/google/calendar_token '' "$DEVICE"

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
