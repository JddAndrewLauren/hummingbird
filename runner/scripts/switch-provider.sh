#!/bin/bash
# Swap hummingbird-runner between first-party Anthropic and a third-party
# Anthropic-compatible provider (#41 decision 2). No deploy needed -- but the
# two credentials are MUTUALLY EXCLUSIVE, and that is the whole reason this
# script exists rather than a line in the runbook: `ANTHROPIC_API_KEY` goes out
# as `x-api-key` and `ANTHROPIC_AUTH_TOKEN` as `Authorization: Bearer`, and with
# both set the client sends both headers and the provider rejects the request.
# So each direction must clear the other side, not merely set its own.
#
#   ./switch-provider.sh anthropic <key-file>
#   ./switch-provider.sh third-party <key-file> <base-url> <model-id>
#
# <key-file> is a mode-600 file holding only the credential, so no secret
# reaches a shell history or any argv but flyctl's. Confirmed live against
# both directions on 2026-08-11 (Anthropic -> Moonshot `kimi-k3`).
set -euo pipefail

FLY=${FLYCTL:-$(command -v flyctl || echo "$HOME/.fly/bin/flyctl")}
APP=hummingbird-runner
# The config is this script's sibling-of-parent, so the script works from any cwd
# and from any worktree without knowing where the checkout lives.
CONFIG="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/fly.toml"

if [ ! -x "$FLY" ]; then
  echo "flyctl not found (looked on PATH and at \$HOME/.fly/bin/flyctl)" >&2
  echo "set FLYCTL=/path/to/flyctl, or install per docs/runner.md" >&2
  exit 2
fi

mode=${1:?usage: switch-provider.sh anthropic|third-party <key-file> [base-url model-id]}
keyfile=${2:?a mode-600 file holding the credential}
# Strip surrounding whitespace: an editor or a PowerShell clipboard read leaves a
# trailing newline, and a bearer token carrying one fails auth in a way that
# looks nothing like a whitespace problem.
key=$(tr -d ' \t\r\n' < "$keyfile")
[ -n "$key" ] || { echo "$keyfile is empty" >&2; exit 2; }

# Clear only what is actually set: `fly secrets unset` errors on an absent name,
# which would make the very first swap (nothing to clear yet) fail.
clear_if_set() {
  local live name present=()
  live=$("$FLY" secrets list --app "$APP" | awk '{print $1}' | tr -d '*')
  for name in "$@"; do
    if echo "$live" | grep -qx "$name"; then present+=("$name"); fi
  done
  if [ ${#present[@]} -gt 0 ]; then
    "$FLY" secrets unset --config "$CONFIG" --app "$APP" "${present[@]}"
  else
    echo "nothing to clear (${*} not set)"
    # Applies the staged set above. Fails harmlessly before the first deploy,
    # when the app has no machines to restart.
    "$FLY" secrets deploy --config "$CONFIG" --app "$APP" || true
  fi
}

case "$mode" in
  anthropic)
    "$FLY" secrets set --config "$CONFIG" --app "$APP" --stage ANTHROPIC_API_KEY="$key"
    clear_if_set ANTHROPIC_BASE_URL ANTHROPIC_AUTH_TOKEN ANTHROPIC_MODEL
    ;;
  third-party)
    # No apostrophes in these messages: inside ${var:?word} a single quote opens
    # a quoted string running to the next one, which silently swallows the line
    # after it (that cost a debugging round when this script was written).
    base=${3:?the Anthropic-compatible endpoint for the provider}
    model=${4:?the model id for the provider}
    "$FLY" secrets set --config "$CONFIG" --app "$APP" --stage \
      ANTHROPIC_AUTH_TOKEN="$key" ANTHROPIC_BASE_URL="$base" ANTHROPIC_MODEL="$model"
    clear_if_set ANTHROPIC_API_KEY
    ;;
  *) echo "unknown mode: $mode" >&2; exit 2 ;;
esac

echo "--- now set ---"
"$FLY" secrets list --app "$APP"
echo
echo "Eyeball a few runs after a swap: the per-skill schema catches shape"
echo "failures, never judgment failures. And set a spend cap in whichever"
echo "provider console holds the key -- nothing here bounds spend."
