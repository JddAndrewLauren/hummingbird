#!/usr/bin/env bash
# Survey + rank helper for the /next-up-hb skill (#116).
#
# Usage: next-up.sh survey [--context X] [--energy low|medium|high]
#                          [--size quick|short|deep] [--calendar <file>]
#                          [--now-local YYYY-MM-DDTHH:MM] [--now-epoch-ms N]
#        next-up.sh rank                 # a prebuilt envelope on stdin
#
# READ-ONLY. `survey` makes exactly one HTTP call, `GET /api/sweep`, and
# there is no write verb here at all -- v1 of this skill touches no write
# route (#116).
#
# Credential posture, worth stating where the token is read rather than
# only in a doc: a `device` token is the authority's **only read-capable
# scope**, and it is write-everything. So this read-only script still
# carries a write credential -- the same posture `server/scripts/
# smoke-prod.sh` records, and the same reason neither ever goes into GitHub
# Actions.
#
# The two verbs exist because the skill has two arms (#41):
#   survey -- the interactive arm, which holds the token and fetches;
#   rank   -- the hosted skill-runner arm, which is context-blind: the
#             sweep payload arrives in the `{skill, args}` request from the
#             calling device's mirror, so the runner holds no authority
#             token and makes no HTTP call.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# .claude/skills/next-up-hb/scripts -> repo root
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../../../.." && pwd)"

API_BASE="${HB_API_BASE:-https://hb.twinion.net}"
TOKEN_PATH="${HB_API_TOKEN_PATH:-$HOME/.config/hummingbird/api-token}"

die() { echo "next-up.sh: $*" >&2; exit 1; }

# The binary is prebuilt in the runner image (ENV HB_NEXT_UP_BIN); in a
# checkout it is built on demand. `cargo run` prints nothing of its own at
# --quiet, so stdout stays pure JSON either way.
rank_envelope() { # envelope JSON on stdin -> ranked JSON on stdout
  if [[ -n "${HB_NEXT_UP_BIN:-}" ]]; then
    "$HB_NEXT_UP_BIN"
  else
    cargo run --quiet --release \
      --manifest-path "$REPO_ROOT/client/next-up/Cargo.toml" \
      --bin next-up-rank
  fi
}

fetch_sweep() {
  # A fixture short-circuits the fetch entirely, so the skill is
  # exercisable with no credential at all.
  if [[ -n "${HB_SWEEP_FIXTURE:-}" ]]; then
    [[ -f "$HB_SWEEP_FIXTURE" ]] || die "no sweep fixture at $HB_SWEEP_FIXTURE"
    cat "$HB_SWEEP_FIXTURE"
    return
  fi

  # Preflight: one provisioning line, non-zero exit, no stack trace and no
  # offer to mint one (`linear.sh`'s discipline).
  [[ -f "$TOKEN_PATH" ]] || die "no authority token at $TOKEN_PATH — mint a device-scope token against $API_BASE (POST /api/admin/tokens, ADMIN_SECRET) and save it to that path"
  local token status body
  token=$(<"$TOKEN_PATH")
  body=$(mktemp)
  # shellcheck disable=SC2064
  trap "rm -f '$body'" RETURN
  status=$(curl -sS -o "$body" -w '%{http_code}' \
    -H "Authorization: Bearer $token" "$API_BASE/api/sweep")
  [[ "$status" == "200" ]] || die "GET $API_BASE/api/sweep answered $status"
  # The SPA shell answers 200 text/html on an unmatched path, so a 200 is
  # not on its own proof the API was reached (smoke-prod.sh's own check).
  jq -e 'has("items")' "$body" >/dev/null 2>&1 || die "GET $API_BASE/api/sweep did not answer a sweep payload"
  cat "$body"
}

cmd="${1:-}"
shift || true

case "$cmd" in
  survey)
    context="" energy="" size="" calendar_file=""
    now_local="$(date +%Y-%m-%dT%H:%M)"
    now_epoch_ms="$(( $(date +%s) * 1000 ))"
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --context)      context="$2"; shift 2 ;;
        --energy)       energy="$2"; shift 2 ;;
        --size)         size="$2"; shift 2 ;;
        --calendar)     calendar_file="$2"; shift 2 ;;
        --now-local)    now_local="$2"; shift 2 ;;
        --now-epoch-ms) now_epoch_ms="$2"; shift 2 ;;
        *) die "unknown option $1" ;;
      esac
    done
    [[ -z "$energy" || "$energy" =~ ^(low|medium|high)$ ]] || die "--energy must be low, medium or high"
    [[ -z "$size" || "$size" =~ ^(quick|short|deep)$ ]] || die "--size must be quick, short or deep"
    [[ -z "$calendar_file" || -f "$calendar_file" ]] || die "no calendar context file at $calendar_file"

    sweep=$(fetch_sweep)
    jq -n \
      --argjson sweep "$sweep" \
      --arg context "$context" --arg energy "$energy" --arg size "$size" \
      --arg nowLocal "$now_local" --argjson nowEpochMs "$now_epoch_ms" \
      --argjson calendar "$([[ -n "$calendar_file" ]] && cat "$calendar_file" || echo null)" \
      '{
         sweep: $sweep,
         axes: ({context: $context, energy: $energy, size: $size}
                | with_entries(select(.value != ""))),
         now: {local: $nowLocal, epoch_ms: $nowEpochMs}
       }
       | if $calendar == null then . else . + {calendar: $calendar} end' \
      | rank_envelope
    ;;

  rank)
    rank_envelope
    ;;

  *)
    die "usage: next-up.sh survey [--context X] [--energy X] [--size X] [--calendar <file>] | next-up.sh rank"
    ;;
esac
