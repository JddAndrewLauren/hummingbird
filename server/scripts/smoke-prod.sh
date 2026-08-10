#!/usr/bin/env bash
# The production deploy-integrity smoke (#240), against the live authority at
# https://hb.twinion.net. Run it by hand after any change that touches
# routing, bindings, secrets or the schema:
#
#   ./scripts/smoke-prod.sh                       # host + token from defaults
#   SMOKE_HOST=https://hb.twinion.net ./scripts/smoke-prod.sh
#   HB_DEVICE_TOKEN=hb_... ./scripts/smoke-prod.sh
#
# This is NOT a second acceptance suite. `server-test.yml` already proves
# handler logic, CAS semantics, the scope matrix and sweep/delta agreement
# against a real SQLite DO before any deploy can land; re-proving them here
# would buy nothing. What CI structurally cannot see is the environment: does
# the /api/* route still beat the shell's SPA fallback, is auth actually on,
# is the live schema the schema we built, does the domain answer JSON. That
# is all this script asserts.
#
# It is READ-ONLY, by decision (#239). Four consequences, each deliberate:
#
#   1. Nothing here writes to the production task database. There is no
#      delete route anywhere in the API (ADR-0003: absence demotes, it never
#      deletes), so a write could never have been cleaned up — only
#      tombstoned with `archived_at`.
#   2. It therefore never exercises the production WRITE path. A deploy that
#      broke writes while leaving reads intact passes this script green. The
#      only proof the live DO is writable is the foreign-device round-trip in
#      #241, done by hand, once. This is a known, accepted gap; #234's "Not
#      yet specified" carries the conditional that closes it.
#   3. The credential is the operator's own `device` token, reused rather
#      than dedicated. `device` is the ONLY read-capable scope (`sweeper` and
#      `ingest` both 403 on reads), so the token this script carries can
#      write everything even though the script does not. The restraint is a
#      property of the script, not of the credential.
#   4. Which is exactly why this is never wired into CI. Putting a
#      read-everything credential into Actions secrets is the posture
#      `deploy-server.yml` refuses for ADMIN_SECRET and FCM_SERVICE_ACCOUNT.
#      Automating this check requires minting a DEDICATED token first — that
#      ordering is the decision, not an implementation detail.
#
# Not asserted here, each for a reason: the scope matrix (needs `sweeper` and
# `ingest` tokens this script deliberately does not hold), the whole admin
# lane (needs ADMIN_SECRET, which stays in the vault), that the database is
# non-empty (production legitimately starts empty — asserting rows exist
# would fail a fresh deploy and pass only by accident of history), and the
# Durable Object's `use_sqlite` flag (readable only via the Cloudflare API
# with the account token, a different credential; #237 verified it once by
# hand and only a rebind could change it).
set -euo pipefail

command -v jq >/dev/null || { echo "FAIL: jq is required" >&2; exit 1; }
command -v curl >/dev/null || { echo "FAIL: curl is required" >&2; exit 1; }

HOST="${SMOKE_HOST:-https://hb.twinion.net}"
TOKEN_FILE="${HB_TOKEN_FILE:-$HOME/.hb-device-token}"

# The token never appears on the command line (where it would land in the
# shell history and in every ps listing) — it arrives in the environment or
# is read from a mode-600 file outside the repo.
TOKEN="${HB_DEVICE_TOKEN:-}"
if [ -z "$TOKEN" ]; then
  [ -r "$TOKEN_FILE" ] || {
    echo "FAIL: no credential — set HB_DEVICE_TOKEN or put the device token in $TOKEN_FILE" >&2
    exit 1
  }
  TOKEN=$(tr -d '[:space:]' <"$TOKEN_FILE")
fi
[ -n "$TOKEN" ] || { echo "FAIL: the device token is empty" >&2; exit 1; }

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# One GET; asserts the status and leaves the body in $BODY and the response's
# content type in $CTYPE. The optional 2nd argument is a bearer token.
#
# Both trailers land on ONE final line, deliberately. The obvious spelling
# puts them on two lines and breaks on exactly the response this script
# checks first: the empty-bodied 401 carries no content-type at all, so the
# trailer is blank, and command substitution strips the trailing newline —
# collapsing two lines into one and shifting the status out of position. One
# line with `read` handles a missing content type as a missing field, which
# is what it is.
get() {
  local expected_status=$1 path=$2 token=${3:-}
  local args=(-s -w '\n%{http_code} %{content_type}' "$HOST$path")
  [ -n "$token" ] && args+=(-H "Authorization: Bearer $token")
  local raw
  raw=$(curl "${args[@]}") || fail "GET $path: curl could not reach $HOST"
  local trailer=${raw##*$'\n'}
  BODY=${raw%$'\n'*}
  local status
  read -r status CTYPE <<<"$trailer"
  CTYPE=${CTYPE:-}
  [ "$status" = "$expected_status" ] ||
    fail "GET $path -> ${status:-<none>} (wanted $expected_status): $BODY"
}

echo "smoke-prod: $HOST"

# ------------------------------------------------------- the route is ours
#
# The most valuable assertion in the file. /api/sweep is precisely a path the
# shell's SPA fallback would answer 200 text/html if route-vs-Custom-Domain
# precedence ever regressed (see server/worker/wrangler.toml's flip
# condition). #238 verified this by hand; here it is made permanent.
get 401 /api/sweep
[ -z "$BODY" ] || fail "the unauthenticated 401 leaked a body: $BODY"

# A garbage credential must be refused the same way. This is what separates
# "auth is on" from "everything 401s for some unrelated reason".
get 401 /api/sweep "not-a-real-token"
[ -z "$BODY" ] || fail "the bad-credential 401 leaked a body: $BODY"

# ------------------------------------------------------------ the sweep
#
# The content type is asserted, not just the status: a 200 text/html here is
# the flip condition firing, and a status-only check would sail past it.
get 200 /api/sweep "$TOKEN"
case "$CTYPE" in
  application/json*) ;;
  *) fail "the sweep answered content-type '$CTYPE' — a non-JSON 200 is the SPA fallback";;
esac
SWEEP=$BODY
jq -e . >/dev/null 2>&1 <<<"$SWEEP" || fail "the sweep body is not valid JSON: $SWEEP"

# Tokens are per-writer machinery and never sync. A leak here would be
# catastrophic and completely silent.
[ "$(jq -r 'has("tokens")' <<<"$SWEEP")" = "false" ] || fail "tokens leaked into the sweep"

# The deployed schema is the schema we built. A SCHEMA_VERSION mismatch or a
# half-applied migration surfaces here and nowhere else in a read-only
# script. Ten lanes, not fourteen: `tokens` and `meta` never appear (the
# latter IS the response's `version`), and push_targets/deliveries are out
# structurally — neither carries a `version` column, so neither can sit on a
# cursor at all (authority/src/handlers/changes.rs). ChangesResponse has no
# skip_serializing_if, so every lane key is emitted even when its table is
# empty, which is what makes presence assertable on an empty database.
for lane in version projects routes fog items steps blocked_by alerts \
            context_snapshots settings rules; do
  [ "$(jq -r --arg k "$lane" 'has($k)' <<<"$SWEEP")" = "true" ] ||
    fail "the sweep is missing the '$lane' lane — the live schema is not the one we built"
done
VERSION=$(jq -r '.version' <<<"$SWEEP")
case "$VERSION" in
  ''|*[!0-9]*) fail "the sweep's version is not a non-negative integer: '$VERSION'";;
esac

# --------------------------------------------- sweep == delta-from-zero
#
# ADR's correctness backstop, asserted in production: two distinct handler
# entry points must agree byte for byte. Unlike the local smoke, this one
# races real traffic — a capture syncing from the operator's phone mid-run
# would legitimately change the answer between the two reads. A differing
# `version` is that race and not a defect, so it is retried once; a
# difference at the SAME version is a genuine disagreement and fails.
compare_sweep_to_delta() {
  local sweep=$1 delta
  get 200 "/api/changes?since=0" "$TOKEN"
  delta=$BODY
  [ "$sweep" = "$delta" ] && return 0
  DELTA_VERSION=$(jq -r '.version' <<<"$delta")
  [ "$DELTA_VERSION" != "$(jq -r '.version' <<<"$sweep")" ] && return 1
  fail "sweep and delta-from-zero differ at the same version:
sweep: $sweep
delta: $delta"
}
if ! compare_sweep_to_delta "$SWEEP"; then
  echo "  (a write landed mid-run; re-reading)"
  get 200 /api/sweep "$TOKEN"
  SWEEP=$BODY
  VERSION=$(jq -r '.version' <<<"$SWEEP")
  compare_sweep_to_delta "$SWEEP" ||
    fail "sweep and delta-from-zero still disagree after a re-read"
fi

# ------------------------------------------------- the cursor is real
#
# Asking for changes since the current version must come back empty at that
# same version — proof the version counter is a live monotonic cursor rather
# than a stub. Same concurrent-write race: a genuine write between the two
# reads makes a non-empty answer correct, and it arrives with a HIGHER
# version, which is how the two cases are told apart.
cursor_is_empty() {
  get 200 "/api/changes?since=$VERSION" "$TOKEN"
  # Declared before assignment on purpose: `local x=$(...)` makes the
  # assignment the command whose status `set -e` sees, so a jq failure would
  # be swallowed and land in the mismatch branch below with a misleading
  # message.
  local at rows
  at=$(jq -r '.version' <<<"$BODY")
  rows=$(jq -r '[.projects,.routes,.fog,.items,.steps,.blocked_by,.alerts,
                 .context_snapshots,.settings,.rules] | map(length) | add' <<<"$BODY")
  if [ "$rows" != "0" ]; then
    [ "$at" -gt "$VERSION" ] && return 1
    fail "since=current returned $rows rows at the unchanged version $VERSION"
  fi
  [ "$at" = "$VERSION" ] ||
    fail "since=current answered version $at, wanted $VERSION"
  return 0
}
if ! cursor_is_empty; then
  echo "  (a write landed mid-run; re-reading the cursor)"
  get 200 /api/sweep "$TOKEN"
  VERSION=$(jq -r '.version' <<<"$BODY")
  cursor_is_empty || fail "the version cursor still does not settle after a re-read"
fi

echo "smoke-prod: all assertions passed (read-only; the write path is unproven by design)"
