#!/bin/bash
# Mint a `device`-scope authority token for one operator device and store it in
# 1Password -- the gesture that provisions a new phone, laptop or emulator, and
# the same one that rotates it later.
#
#   ./scripts/mint-device-token.sh <token-id> <device name> <1password title>
#   ./scripts/mint-device-token.sh device-mac "Johns-MBP (Mac shell)" hummingbird-device-mac
#
# This is a third mint script rather than a flag on the other two because the
# *sink* differs, and the sink is the whole risk. `scripts/mint-hb-token.sh`
# (sweeper) and `runner/scripts/mint-hb-token.sh` (runner) both hand the
# plaintext to `fly secrets set` for an app they own; a device token has no Fly
# app to land in -- its home is the vault, and a human copies it from there onto
# the device. Everything those two headers say about *why* a mint is a script
# applies here verbatim: the plaintext exists only in the original 201.
# `POST /api/admin/tokens` is idempotent by the client-supplied `id` and stores
# only a hash, so a replay answers 200 with the metadata and no token, and a
# lost plaintext burns that id permanently -- `revoke` is a soft delete that
# does not free it for re-minting.
#
# **The store is rehearsed and the mint is not.** Provisioning the Mac on
# 2026-08-20 burnt `device-mac` and `device-mac-2` back to back: both mints
# returned 201 and both `op item create` calls then failed -- first because zsh
# glob-expanded `password[password]=-` before `op` ran, then because `op item
# create` reads piped input *only* as a JSON item template, so `field=-` is not
# a stdin form at all. Hence the shape below: a JSON template on stdin (which
# also keeps the plaintext out of argv, where the assignment form would put
# it), a readback that compares byte-for-byte before declaring success, and
# bash rather than an interactive zsh one-liner. If you edit the store, rehearse
# the edited command with a dummy value into a throwaway item first. A failed
# store is only recoverable by burning another id.
#
# ADMIN_SECRET comes from 1Password (`HB_ADMIN_REF` overrides the reference);
# it mints every other token, so it never goes near Actions -- see CLAUDE.md,
# "Credential blast radius". HB_API_BASE overrides the authority (default
# https://hb.twinion.net).
#
# A device token is the authority's only read-capable scope AND is
# write-everything -- and since #273 it can cause model spend, and since #577 it
# can mint a Google calendar token. Anything holding one is a write credential
# however read-only it looks.
set -euo pipefail

id=${1:?usage: mint-device-token.sh <token-id> <device name> <1password title>}
name=${2:?usage: mint-device-token.sh <token-id> <device name> <1password title>}
title=${3:?usage: mint-device-token.sh <token-id> <device name> <1password title>}

API_BASE=${HB_API_BASE:-https://hb.twinion.net}
ADMIN_REF=${HB_ADMIN_REF:-op://dev/hummingbird-authority-admin/ADMIN_SECRET}

for cmd in curl op python3; do
  command -v "$cmd" > /dev/null || { echo "$cmd not found on PATH" >&2; exit 2; }
done

# An `op read` that hangs means the approval prompt went unanswered, not that
# auth failed -- so this runs first and alone, before anything is minted.
admin=$(op read "$ADMIN_REF")
[ -n "$admin" ] || { echo "$ADMIN_REF is empty" >&2; exit 2; }

# The secret is handed to curl through a stdin config file below, whose format
# is `header = "..."` with backslash escaping. A `"` or `\` in the secret would
# be swallowed by that quoting and send a silently wrong credential -- a 401
# that looks like a bad ADMIN_SECRET rather than a bad parse. The authority
# mints hex, so this never fires; refuse rather than mis-send if it ever does.
case "$admin" in
  *[\"\\]*)
    echo "ADMIN_SECRET contains a quote or backslash, which this script cannot" >&2
    echo "pass to curl without ambiguity. Re-mint it without those characters." >&2
    exit 2 ;;
esac

# Refuse before minting if the vault title is taken: `op item create` would
# happily make a second item with the same title, and `op read` on an ambiguous
# title is then a coin toss over which token the next device gets.
if op item get "$title" > /dev/null 2>&1; then
  echo "1Password already holds an item titled '$title' -- pick another title," >&2
  echo "or delete that one first. Nothing has been minted." >&2
  exit 2
fi

# No `source` field. It is *required* for an ingest token and *forbidden* for
# every other scope (handlers/admin_tokens.rs, per ADR-0008); sending one here
# is a 400, not a harmless extra.
body=$(python3 -c '
import json, sys
print(json.dumps({"id": sys.argv[1], "name": sys.argv[2], "scope": "device"}))' "$id" "$name")

resp=$(mktemp)
# shellcheck disable=SC2064  # expand $resp now: it must be removed even if reassigned
trap "rm -f '$resp'" EXIT

# The Authorization header goes in through `--config -` rather than `-H`, so
# ADMIN_SECRET never appears in curl's argv where any local process could read
# it out of `ps`. `printf` is a bash builtin, so the pipeline forks nothing that
# carries the secret either, and nothing touches disk.
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

# `hb_` + 64 hex chars (handlers/admin_tokens.rs). An empty or truncated value
# stored here would look exactly like a working provision until the device
# failed to auth, so refuse to store anything of the wrong shape.
case "$token" in
  hb_????????????????????????????????????????????????????????????????) ;;
  *)
    echo "the 201 carried a token of unexpected shape (length ${#token}) -- not storing it." >&2
    echo "id '$id' is now spent; revoke it and re-run under a fresh id." >&2
    exit 1 ;;
esac

# A JSON item template on stdin -- see the header. The assignment form would
# both glob under zsh and put the plaintext in argv.
TOKEN="$token" TITLE="$title" \
NOTE="Device token for $name (authority id '$id'). Revoke: DELETE $API_BASE/api/admin/tokens/$id with ADMIN_SECRET." \
python3 -c '
import json, os
print(json.dumps({"title": os.environ["TITLE"], "category": "PASSWORD", "fields": [
    {"id": "password", "type": "CONCEALED", "purpose": "PASSWORD", "value": os.environ["TOKEN"]},
    {"id": "notesPlain", "type": "STRING", "purpose": "NOTES", "value": os.environ["NOTE"]}]}))' \
  | op item create --vault "${HB_VAULT:-dev}" - > /dev/null

# Read it back and compare, rather than trusting the exit code: a stored-but-
# wrong value is indistinguishable from a good one until the device fails auth,
# and by then the plaintext is unrecoverable.
back=$(op read "op://${HB_VAULT:-dev}/$title/password")
if [ "$back" != "$token" ]; then
  echo "the vault copy does not match what was minted -- id '$id' is spent and its" >&2
  echo "plaintext is now lost. Revoke it and re-run under a fresh id." >&2
  exit 1
fi

echo "minted device-scope token '$id' and verified it at op://${HB_VAULT:-dev}/$title/password"
echo "The authority cannot return this plaintext again; revoke it with"
echo "DELETE /api/admin/tokens/$id."
echo "Copy it onto the device from 1Password -- never reuse another device's"
echo "token, or last_seen can no longer tell the two apart."
