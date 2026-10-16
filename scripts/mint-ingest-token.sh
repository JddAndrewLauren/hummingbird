#!/bin/bash
# Mint an `ingest`-scope authority token bound to one registered source and
# store it in 1Password -- the gesture that credentials a pushed source (the
# Starling cockpit's `starling/v1`, #830) whose sender reads its token from
# the vault rather than from a Fly app this repo owns.
#
#   ./scripts/mint-ingest-token.sh <token-id> <source> <1password title> [allowed-origin]
#   ./scripts/mint-ingest-token.sh starling-ingest starling/v1 hummingbird-starling-ingest
#   ./scripts/mint-ingest-token.sh perch-ingest perch/v1 perch-hummingbird-ingest https://hb.twinion.net
#   ./scripts/mint-ingest-token.sh healthchecks-ingest healthchecks/v1 hummingbird-healthchecks-ingest
#
# This is `scripts/mint-device-token.sh` with a different scope, and it is a
# fourth script rather than a flag on that one for the reason that header gives
# for being a third: the store is the risk, and that store is rehearsed. A
# scope flag would put an untested branch inside the one script every new
# device is provisioned through. Everything that header says applies here
# verbatim -- the plaintext exists only in the original 201, a replay answers
# 200 with no token, a lost plaintext burns the id for good, and the store is a
# JSON template on stdin with a byte-for-byte readback. The store below is that
# script's, unchanged but for the note text.
#
# The source must be in the frozen registry (server/domain/src/sources.rs). The
# authority checks it at mint and answers 400 naming the source, before any row
# is written -- so a typo here costs nothing, not an id.
#
# The optional fourth argument is the token's allowed link origin (ADR-0014's
# #845 amendment): an alert the token writes whose `url` is off that origin is
# refused. A source that requires one (`auk/v1`, `perch/v1`) is refused at mint
# without it, and so is anything that is not an https origin -- scheme, host and
# optional port, nothing else -- both as a 400 before any row is written. The
# authority stores it canonical on the token's row and reports it in the token
# listing; nothing here keeps a copy.
#
# ADMIN_SECRET comes from 1Password (`HB_ADMIN_REF` overrides the reference);
# it mints every other token, so it never goes near Actions -- see CLAUDE.md,
# "Credential blast radius". HB_API_BASE overrides the authority (default
# https://hb.twinion.net), HB_VAULT the vault (default dev).
#
# An ingest token can raise, re-raise and resolve alerts (and, where the source
# declares it, write snapshots) under its one source, and nothing else: no
# reads, no items, no spend. That bounded, local worst case is why it belongs in
# `dev` -- revoke it and the source goes quiet.
set -euo pipefail

usage='usage: mint-ingest-token.sh <token-id> <source> <1password title> [allowed-origin]'
id=${1:?$usage}
source=${2:?$usage}
title=${3:?$usage}
origin=${4:-}

API_BASE=${HB_API_BASE:-https://hb.twinion.net}
ADMIN_REF=${HB_ADMIN_REF:-op://dev/hummingbird-authority-admin/ADMIN_SECRET}
VAULT=${HB_VAULT:-dev}

for cmd in curl op python3; do
  command -v "$cmd" > /dev/null || { echo "$cmd not found on PATH" >&2; exit 2; }
done

# An `op read` that hangs means the approval prompt went unanswered, not that
# auth failed -- so this runs first and alone, before anything is minted.
admin=$(op read "$ADMIN_REF")
[ -n "$admin" ] || { echo "$ADMIN_REF is empty" >&2; exit 2; }

# See mint-device-token.sh: the secret goes to curl through a stdin config file
# whose quoting would swallow a `"` or `\`.
case "$admin" in
  *[\"\\]*)
    echo "ADMIN_SECRET contains a quote or backslash, which this script cannot" >&2
    echo "pass to curl without ambiguity. Re-mint it without those characters." >&2
    exit 2 ;;
esac

# Refuse before minting if the vault title is taken: a second item with the
# same title makes `op read` a coin toss over which token the sender gets.
if op item get "$title" > /dev/null 2>&1; then
  echo "1Password already holds an item titled '$title' -- pick another title," >&2
  echo "or delete that one first. Nothing has been minted." >&2
  exit 2
fi

# `source` is *required* for an ingest token and forbidden for every other
# scope (handlers/admin_tokens.rs, per ADR-0008). The token's name is its id:
# the source is already on the row, and the id is what a revoke names. The
# origin goes in only when given: an absent field, never an empty string.
body=$(python3 -c '
import json, sys
mint = {"id": sys.argv[1], "name": sys.argv[1], "scope": "ingest", "source": sys.argv[2]}
if sys.argv[3]:
    mint["allowed_origin"] = sys.argv[3]
print(json.dumps(mint))' \
  "$id" "$source" "$origin")

resp=$(mktemp)
# shellcheck disable=SC2064  # expand $resp now: it must be removed even if reassigned
trap "rm -f '$resp'" EXIT

# The Authorization header goes in through `--config -` rather than `-H`, so
# ADMIN_SECRET never appears in curl's argv.
code=$(printf 'header = "Authorization: Bearer %s"\n' "$admin" \
  | curl -sS -o "$resp" -w '%{http_code}' --config - \
    -X POST "$API_BASE/api/admin/tokens" \
    -H 'content-type: application/json' \
    -d "$body")

case "$code" in
  201) ;;
  200)
    echo "id '$id' has already been minted: the authority answered 200 with the" >&2
    echo "metadata and no token. That plaintext is gone for good -- revoke it with" >&2
    echo "DELETE /api/admin/tokens/$id and re-run under a fresh id." >&2
    exit 1 ;;
  400)
    echo "the authority refused the mint (HTTP 400) -- nothing was minted, so '$id' is" >&2
    echo "still free. An unregistered or retired source answers here, as does a" >&2
    echo "missing or non-https origin:" >&2
    cat "$resp" >&2
    echo >&2
    exit 1 ;;
  401|403)
    echo "the authority rejected ADMIN_SECRET (HTTP $code)." >&2
    echo "an unconfigured ADMIN_SECRET fails closed the same way -- check the server, not just the vault." >&2
    exit 1 ;;
  *)
    echo "mint failed: HTTP $code" >&2
    cat "$resp" >&2
    echo >&2
    exit 1 ;;
esac

token=$(python3 -c '
import json, sys
print(json.load(open(sys.argv[1]))["token"])' "$resp")

# `hb_` + 64 hex chars (handlers/admin_tokens.rs). Refuse to store anything of
# the wrong shape: it would look like a working credential until the sender
# failed to auth.
case "$token" in
  hb_????????????????????????????????????????????????????????????????) ;;
  *)
    echo "the 201 carried a token of unexpected shape (length ${#token}) -- not storing it." >&2
    echo "id '$id' is now spent; revoke it and re-run under a fresh id." >&2
    exit 1 ;;
esac

bound="bound to $source"
[ -z "$origin" ] || bound="$bound, links held to $origin"

# The JSON item template on stdin, exactly as mint-device-token.sh stores it.
TOKEN="$token" TITLE="$title" \
NOTE="Ingest token $bound (authority id '$id'). Revoke: DELETE $API_BASE/api/admin/tokens/$id with ADMIN_SECRET." \
python3 -c '
import json, os
print(json.dumps({"title": os.environ["TITLE"], "category": "PASSWORD", "fields": [
    {"id": "password", "type": "CONCEALED", "purpose": "PASSWORD", "value": os.environ["TOKEN"]},
    {"id": "notesPlain", "type": "STRING", "purpose": "NOTES", "value": os.environ["NOTE"]}]}))' \
  | op item create --vault "$VAULT" - > /dev/null

# Read it back and compare, rather than trusting the exit code: a stored-but-
# wrong value is indistinguishable from a good one until the sender fails auth,
# and by then the plaintext is unrecoverable.
back=$(op read "op://$VAULT/$title/password")
if [ "$back" != "$token" ]; then
  echo "the vault copy does not match what was minted -- id '$id' is spent and its" >&2
  echo "plaintext is now lost. Revoke it and re-run under a fresh id." >&2
  exit 1
fi

echo "minted ingest token '$id' $bound and verified it at op://$VAULT/$title/password"
echo "The authority cannot return this plaintext again; revoke it with"
echo "DELETE /api/admin/tokens/$id."
