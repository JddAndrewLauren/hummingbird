#!/usr/bin/env bash
# The wire-level half of #113's acceptance: boots `wrangler dev` against the
# real SQLite-backed Durable Object and asserts the create / replay / CAS /
# version-gated-read behaviours over HTTP. The statement-level criterion
# ("no items scan on an unchanged workspace") is proven by the native
# RecordingSql test, not here — the wire can only show emptiness.
#
# Run from server/: ./scripts/smoke.sh
set -euo pipefail

PORT="${SMOKE_PORT:-8787}"
BASE="http://127.0.0.1:${PORT}"
cd "$(dirname "$0")/../worker"

# A stale local dev DB would break replay/version assertions.
rm -rf .wrangler/state

export CI=true WRANGLER_SEND_METRICS=false
npx --yes wrangler@4 dev --port "$PORT" >/tmp/wrangler-smoke.log 2>&1 &
WRANGLER_PID=$!
trap 'kill "$WRANGLER_PID" 2>/dev/null || true' EXIT

# First boot compiles the worker and downloads workerd — be generous.
for _ in $(seq 1 60); do
  code=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/api/changes?since=0" || true)
  [ "$code" = "200" ] && break
  sleep 3
done
if [ "${code:-}" != "200" ]; then
  echo "FAIL: wrangler dev never became ready; last 40 log lines:" >&2
  tail -40 /tmp/wrangler-smoke.log >&2
  exit 1
fi

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# One request; asserts the status and leaves the body in $BODY.
request() {
  local expected_status=$1 method=$2 path=$3 data=${4:-}
  local args=(-s -w '\n%{http_code}' -X "$method" "$BASE$path")
  [ -n "$data" ] && args+=(-d "$data")
  local raw
  raw=$(curl "${args[@]}")
  BODY=$(printf '%s' "$raw" | head -n -1)
  local status
  status=$(printf '%s' "$raw" | tail -n 1)
  [ "$status" = "$expected_status" ] ||
    fail "$method $path -> $status (wanted $expected_status): $BODY"
}

# Create -> 201, server-stamped version.
request 201 POST /api/items '{"id":"smoke-1","title":"hello"}'
V1=$(jq -r '.version' <<<"$BODY")
[ "$(jq -r '.seq' <<<"$BODY")" = "1" ] || fail "create did not mint seq 1: $BODY"

# Replay of the identical create -> 200, same row, no duplicate.
request 200 POST /api/items '{"id":"smoke-1","title":"hello"}'
[ "$(jq -r '.version' <<<"$BODY")" = "$V1" ] || fail "replay bumped version: $BODY"
request 200 GET "/api/changes?since=0"
[ "$(jq -r '.items | length' <<<"$BODY")" = "1" ] || fail "replay duplicated the row: $BODY"

# Stale expected_version -> 409 carrying the current entity.
request 409 PATCH /api/items/smoke-1 '{"expected_version":999999,"title":"x"}'
[ "$(jq -r '.error' <<<"$BODY")" = "version_conflict" ] || fail "409 body shape: $BODY"
[ "$(jq -r '.current.version' <<<"$BODY")" = "$V1" ] || fail "409 lacks current entity: $BODY"
[ "$(jq -r '.current.title' <<<"$BODY")" = "hello" ] || fail "409 entity was modified: $BODY"

# Fresh expected_version -> applies and bumps.
request 200 PATCH /api/items/smoke-1 "{\"expected_version\":$V1,\"title\":\"renamed\"}"
V2=$(jq -r '.version' <<<"$BODY")
[ "$V2" -gt "$V1" ] || fail "fresh patch did not bump version: $BODY"
[ "$(jq -r '.title' <<<"$BODY")" = "renamed" ] || fail "fresh patch did not apply: $BODY"

# Version-gated read: current cursor -> empty; older cursor -> the change.
request 200 GET "/api/changes?since=$V2"
[ "$(jq -r '.items | length' <<<"$BODY")" = "0" ] || fail "since=current not empty: $BODY"
[ "$(jq -r '.version' <<<"$BODY")" = "$V2" ] || fail "since=current wrong version: $BODY"
request 200 GET "/api/changes?since=$V1"
[ "$(jq -r '.items | length' <<<"$BODY")" = "1" ] || fail "since=V1 wrong count: $BODY"
[ "$(jq -r '.items[0].title' <<<"$BODY")" = "renamed" ] || fail "since=V1 wrong row: $BODY"

echo "smoke: all assertions passed"
