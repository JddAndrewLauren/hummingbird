#!/bin/bash
# Mint the sweeper's `sweeper`-scope authority token and stage it as
# `HB_API_TOKEN` on hummingbird-sweeper -- the credential the retargeted write
# side needs (docs/sweeper.md, "Go-live after the retarget (#123)" step 1), and
# the same gesture that rotates it later.
#
# This is a script rather than a runbook curl for the reason
# runner/scripts/mint-hb-token.sh gives, which applies verbatim here: the
# plaintext exists only in the original 201. `POST /api/admin/tokens` is
# idempotent by the client-supplied `id` and stores only a hash, so a replay
# answers 200 with the metadata and no token -- unrecoverably. So the mint and
# the `fly secrets set` that consumes it have to happen in one pass, without the
# plaintext ever reaching a shell history, an argv but flyctl's, or a terminal.
#
#   ./scripts/mint-hb-token.sh <admin-secret-file> [token-id]
#
# <admin-secret-file> is a mode-600 file holding only `ADMIN_SECRET` -- which
# mints every other token and therefore never goes near Actions (CLAUDE.md,
# "Credential blast radius"). HB_API_BASE overrides the authority (default
# https://hb.twinion.net).
#
# **Set HB_TOKEN_OUT=<path>.** It writes the minted plaintext there mode-600, for
# moving into 1Password. It is optional in the runner's copy and effectively
# mandatory in this one: the mint takes the `id` the moment it succeeds, so if
# the `fly secrets set` below fails, `set -e` exits and the only copy of the
# plaintext dies with the process -- while a replay under that id now answers 200
# with no token. HB_TOKEN_OUT turns that from unrecoverable into inconvenient.
#
# Differences from the runner's copy, both deliberate:
#   * scope is `sweeper`, which reaches `POST /api/items` and nothing else --
#     the narrowest credential in the system after the ingest tokens. It is
#     still a write credential (docs/sweeper.md, "Secrets").
#   * the secret is **staged**, not applied. The sweeper's machine is stopped
#     until go-live and supercronic starts ticking the moment it comes up, so
#     the operator owns that moment -- not a restart Fly schedules as a side
#     effect of setting a secret.
set -euo pipefail

FLY=${FLYCTL:-$(command -v flyctl || echo "$HOME/.fly/bin/flyctl")}
APP=hummingbird-sweeper
# Sibling-of-parent, so this works from any cwd and any worktree.
CONFIG="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/fly.toml"
API_BASE=${HB_API_BASE:-https://hb.twinion.net}

if [ ! -x "$FLY" ]; then
  echo "flyctl not found (looked on PATH and at \$HOME/.fly/bin/flyctl)" >&2
  echo "set FLYCTL=/path/to/flyctl, or install per docs/sweeper.md" >&2
  exit 2
fi

secretfile=${1:?usage: mint-hb-token.sh <admin-secret-file> [token-id]}
token_id=${2:-sweeper}

# Trim leading and trailing whitespace only, exactly as the runner's copy does
# and for the same reason: an editor or a PowerShell clipboard read leaves a
# trailing newline, and a bearer credential carrying one fails auth in a way
# that looks nothing like a whitespace problem.
admin=$(< "$secretfile")
admin=${admin#"${admin%%[![:space:]]*}"}
admin=${admin%"${admin##*[![:space:]]}"}
[ -n "$admin" ] || { echo "$secretfile is empty" >&2; exit 2; }

# The secret is handed to curl through a stdin config file below, whose format
# is `header = "..."` with backslash escaping. A `"` or `\` in the secret would
# be swallowed by that quoting and send a silently wrong credential -- which
# arrives as a 401 that looks like a bad ADMIN_SECRET rather than a bad parse.
# The authority mints hex, so this never fires; refuse rather than mis-send if
# it ever does.
case "$admin" in
  *[\"\\]*)
    echo "ADMIN_SECRET contains a quote or backslash, which this script cannot" >&2
    echo "pass to curl without ambiguity. Re-mint it without those characters." >&2
    exit 2 ;;
esac

# No `source` field. It is *required* for an ingest token and *forbidden* for
# every other scope (handlers/admin_tokens.rs, per ADR-0008); sending one here
# is a 400, not a harmless extra.
body=$(printf '{"id":"%s","name":"hummingbird-sweeper","scope":"sweeper"}' "$token_id")

resp=$(mktemp)
# shellcheck disable=SC2064  # expand $resp now: it must be removed even if reassigned
trap "rm -f '$resp'" EXIT

# The Authorization header goes in through `--config -` rather than `-H`, so
# ADMIN_SECRET never appears in curl's argv where any local process could read
# it out of `ps`. `printf` is a bash builtin, so the pipeline forks nothing that
# carries the secret either, and nothing touches disk. This is what the header's
# "an argv but flyctl's" claim rests on -- an `-H` here would quietly break it.
#
# One `-w` trailer on one line: two of them on two lines break on any empty
# last field.
code=$(printf 'header = "Authorization: Bearer %s"\n' "$admin" \
  | curl -sS -o "$resp" -w '%{http_code}' --config - \
    -X POST "$API_BASE/api/admin/tokens" \
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

# Grep rather than a JSON parser, as the runner's copy does: the token is
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

echo "minted a sweeper-scope token as id '$token_id'; staging HB_API_TOKEN"
# `--stage`, unlike the runner's copy: see the header. Nothing applies until the
# next deploy of this app.
"$FLY" secrets set --stage --config "$CONFIG" --app "$APP" "HB_API_TOKEN=$token"

echo
echo "staged. Nothing has changed on the running app yet, and the machine is"
echo "still stopped. The next deploy applies this -- and leaves the machine"
echo "stopped: a deploy updates a stopped machine in place and does not start"
echo "it, so starting supercronic is always its own explicit act."
echo "Remaining before that deploy: unset LINEAR_API_KEY (also --stage), set"
echo "GOOGLE_REFRESH_TOKEN and GMAIL_HEALTHCHECK_URL. Unpause both checks"
echo "before the machine start, not after -- see docs/sweeper.md step 7."
echo "The authority cannot return this plaintext again; revoke it with"
echo "DELETE /api/admin/tokens/$token_id."
