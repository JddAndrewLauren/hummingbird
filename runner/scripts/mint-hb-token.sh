#!/bin/bash
# Mint the runner's device-scope authority token and set it as `HB_API_TOKEN` on
# hummingbird-runner -- the one credential `microtask` needs (runbook step 3),
# and the same gesture that rotates it later (step 6).
#
# This is a script rather than a runbook curl for one reason: the plaintext
# exists only in the original 201. `POST /api/admin/tokens` is idempotent by the
# client-supplied `id` and stores only a hash, so a replay answers 200 with the
# metadata and no token -- unrecoverably. So the mint and the `fly secrets set`
# that consumes it have to happen in one pass, without the plaintext ever
# reaching a shell history, an argv but flyctl's, or a terminal.
#
#   ./mint-hb-token.sh <admin-secret-file> [token-id]
#
# <admin-secret-file> is a mode-600 file holding only `ADMIN_SECRET` -- which
# mints every other token and therefore never goes near Actions (CLAUDE.md,
# "Credential blast radius"). Set HB_TOKEN_OUT=<path> to also write the minted
# plaintext there mode-600, for moving into 1Password; otherwise the Fly secret
# becomes its only copy. HB_API_BASE overrides the authority (default
# https://hb.twinion.net).
set -euo pipefail

FLY=${FLYCTL:-$(command -v flyctl || echo "$HOME/.fly/bin/flyctl")}
APP=hummingbird-runner
# Sibling-of-parent, so this works from any cwd and any worktree.
CONFIG="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/fly.toml"
API_BASE=${HB_API_BASE:-https://hb.twinion.net}

if [ ! -x "$FLY" ]; then
  echo "flyctl not found (looked on PATH and at \$HOME/.fly/bin/flyctl)" >&2
  echo "set FLYCTL=/path/to/flyctl, or install per docs/runner.md" >&2
  exit 2
fi

secretfile=${1:?usage: mint-hb-token.sh <admin-secret-file> [token-id]}
token_id=${2:-runner}

# Trim leading and trailing whitespace only, exactly as switch-provider.sh does
# and for the same reason: an editor or a PowerShell clipboard read leaves a
# trailing newline, and a bearer credential carrying one fails auth in a way
# that looks nothing like a whitespace problem.
admin=$(< "$secretfile")
admin=${admin#"${admin%%[![:space:]]*}"}
admin=${admin%"${admin##*[![:space:]]}"}
[ -n "$admin" ] || { echo "$secretfile is empty" >&2; exit 2; }

# No `source` field. It is *required* for an ingest token and *forbidden* for
# every other scope (handlers/admin_tokens.rs, per ADR-0008); sending one here
# is a 400, not a harmless extra.
body=$(printf '{"id":"%s","name":"hummingbird-runner","scope":"device"}' "$token_id")

resp=$(mktemp)
# shellcheck disable=SC2064  # expand $resp now: it must be removed even if reassigned
trap "rm -f '$resp'" EXIT

# One `-w` trailer on one line: two of them on two lines break on any empty
# last field.
code=$(curl -sS -o "$resp" -w '%{http_code}' \
  -X POST "$API_BASE/api/admin/tokens" \
  -H "Authorization: Bearer $admin" \
  -H 'content-type: application/json' \
  -d "$body")

case "$code" in
  200|201) ;;
  401|403)
    echo "the authority rejected ADMIN_SECRET (HTTP $code)." >&2
    echo "an unconfigured ADMIN_SECRET fails closed the same way -- check the server, not just the file." >&2
    exit 1 ;;
  *)
    echo "mint failed: HTTP $code" >&2
    cat "$resp" >&2
    echo >&2
    exit 1 ;;
esac

# Grep rather than a JSON parser, as switch-provider.sh does: the token is
# `hb_` + 64 hex chars, so no JSON escaping can appear inside it and this holds
# whether the response is pretty-printed or not.
token=$(grep -o '"token": *"[^"]*"' "$resp" | cut -d'"' -f4 || true)
if [ -z "$token" ]; then
  echo "HTTP $code with no token in the body: id '$token_id' already exists, so this" >&2
  echo "was an idempotent replay and that plaintext is gone for good. Either set the" >&2
  echo "token you already hold, or revoke and re-mint under the same id:" >&2
  echo "  curl -X DELETE $API_BASE/api/admin/tokens/$token_id -H 'Authorization: Bearer <ADMIN_SECRET>'" >&2
  exit 1
fi

if [ -n "${HB_TOKEN_OUT:-}" ]; then
  (umask 077; printf '%s' "$token" > "$HB_TOKEN_OUT")
  echo "plaintext written to $HB_TOKEN_OUT (mode 600) -- move it into 1Password, then delete it"
fi

echo "minted a device-scope token as id '$token_id'; setting HB_API_TOKEN"
# Not `--stage`: there is nothing to clear afterwards, so this should apply and
# restart the machines now. `HB_API_BASE` is deliberately not set -- the runner
# defaults to https://hb.twinion.net and setting it is a separate, explicit act.
"$FLY" secrets set --config "$CONFIG" --app "$APP" "HB_API_TOKEN=$token"

echo
echo "microtask is live once the machines come back. The authority cannot return"
echo "this plaintext again; revoke it with DELETE /api/admin/tokens/$token_id."
echo "A device token is read-capable AND write-everything -- treat the app as"
echo "holding a write credential from here on (CLAUDE.md, credential blast radius)."
