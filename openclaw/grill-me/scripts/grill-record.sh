#!/usr/bin/env bash
# Grill recorder for the OpenClaw `grill-me` skill (ADR-0029).
#
# Usage: grill-record.sh record <ref> --verdict resolved|fog_remains \
#          --summary <text> --transcript-file <path> \
#          [--proposal <text>] [--applied-patch <text>] [--delete-unticked-plan]
#
# Scope guard, enforced here and not only in prose: this script has exactly
# one verb and it writes exactly one thing — `POST /api/grills`, the route
# that applies a completed grill (ADR-0023). The verdict→stage move and the
# optional unticked-plan delete happen **server-side inside that route**,
# atomically against the item's `expected_version`; this script never
# patches an item or a step itself. Item-field edits the interview turned
# up are applied *before* calling this, through the `hummingbird-tasks`
# skill — this script reads the item's version fresh at record time, so
# edits-then-record is the order that cannot conflict with itself.
#
# The id is deterministic — sha256(namespace + item + "/" + transcript),
# same recipe as microtask's step ids, own frozen namespace — so a retried
# record of the same interview lands on the route's already-exists path
# (200, the stored row) instead of minting a second grill. Changing the
# namespace re-mints every grill this arm has ever recorded.
set -euo pipefail

command -v jq >/dev/null || { echo "grill-record.sh: jq is required" >&2; exit 1; }
command -v curl >/dev/null || { echo "grill-record.sh: curl is required" >&2; exit 1; }

API_BASE="${HB_API_BASE:-https://hb.twinion.net}"
TOKEN_PATH="${HB_API_TOKEN_PATH:-$HOME/.config/hummingbird/api-token}"

ID_NAMESPACE="hummingbird-skill/grill-me/openclaw/v1"

die() { echo "grill-record.sh: $*" >&2; exit 1; }

token() {
  [[ -f "$TOKEN_PATH" ]] || die "no authority token at $TOKEN_PATH — mint a device-scope token against $API_BASE (POST /api/admin/tokens, ADMIN_SECRET) and save it to that path"
  tr -d '[:space:]' <"$TOKEN_PATH"
}

request() { # method path [json-body]
  local method=$1 path=$2 data=${3:-} raw auth_token curl_status
  auth_token=$(token)
  [[ -n "$auth_token" ]] || die "empty authority token at $TOKEN_PATH"
  # `-H @-` reads the header off stdin, through a pipe: the token reaches
  # curl without a path anyone can open and without an argv `ps` can read. A
  # tempfile would satisfy the second and not the first — a signal between
  # writing it and unlinking it strands a live credential in /tmp.
  local args=(-sS -w '\n%{http_code}' --connect-timeout 10 --max-time 30
              -X "$method" -H @- "$API_BASE$path")
  [[ -n "$data" ]] && args+=(-H 'Content-Type: application/json' -d "$data")
  if raw=$(printf 'Authorization: Bearer %s\n' "$auth_token" | curl "${args[@]}"); then
    :
  else
    curl_status=$?
    return "$curl_status"
  fi
  BODY=${raw%$'\n'*}
  STATUS=${raw##*$'\n'}
}

sha256_hex() {
  if command -v sha256sum >/dev/null; then sha256sum | cut -d' ' -f1
  else shasum -a 256 | cut -d' ' -f1; fi
}

deterministic_id() { # seed -> uuid (v4-shaped; hb.sh's recipe, own namespace)
  local hex b6 b8
  hex=$(printf '%s%s' "$ID_NAMESPACE" "$1" | sha256_hex)
  hex=${hex:0:32}
  b6=$(( 0x${hex:12:2} & 0x0F | 0x40 ))
  b8=$(( 0x${hex:16:2} & 0x3F | 0x80 ))
  hex="${hex:0:12}$(printf '%02x' "$b6")${hex:14:2}$(printf '%02x' "$b8")${hex:18:14}"
  printf '%s-%s-%s-%s-%s' "${hex:0:8}" "${hex:8:4}" "${hex:12:4}" "${hex:16:4}" "${hex:20:12}"
}

SWEEP_CACHE=$(mktemp)
trap 'rm -f "$SWEEP_CACHE"' EXIT

fetch_sweep() {
  if [[ ! -s "$SWEEP_CACHE" ]]; then
    request GET /api/sweep
    [[ "$STATUS" == "200" ]] || die "GET /api/sweep answered $STATUS: $BODY"
    jq -e 'has("items")' <<<"$BODY" >/dev/null 2>&1 || die "GET /api/sweep did not answer a sweep payload"
    printf '%s' "$BODY" >"$SWEEP_CACHE"
  fi
  cat "$SWEEP_CACHE"
}

resolve_ref() { # HB-42 | uuid -> uuid
  local ref=$1
  if [[ "$ref" =~ ^[Hh][Bb]-([0-9]+)$ ]]; then
    fetch_sweep | jq -er --argjson seq "${BASH_REMATCH[1]}" \
      'first(.items[] | select(.seq == $seq) | .id) // empty' \
      || die "no item with seq ${BASH_REMATCH[1]} in the sweep"
  else
    fetch_sweep | jq -er --arg id "$ref" 'first(.items[] | select(.id == $id) | .id) // empty' \
      || die "no item $ref in the sweep"
  fi
}

cmd="${1:-}"
shift || true

case "$cmd" in
  record)
    ref="${1:?usage: grill-record.sh record <ref> --verdict ... --summary ... --transcript-file ...}"
    shift
    verdict="" summary="" transcript_file="" proposal="" applied_patch="" delete_plan=false
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --verdict)             verdict="${2:-}"; shift 2 ;;
        --summary)             summary="${2:-}"; shift 2 ;;
        --transcript-file)     transcript_file="${2:-}"; shift 2 ;;
        --proposal)            proposal="${2:-}"; shift 2 ;;
        --applied-patch)       applied_patch="${2:-}"; shift 2 ;;
        --delete-unticked-plan) delete_plan=true; shift ;;
        *) die "unknown flag $1" ;;
      esac
    done
    [[ "$verdict" == "resolved" || "$verdict" == "fog_remains" ]] \
      || die "--verdict must be resolved or fog_remains"
    [[ -n "$summary" ]] || die "--summary is required"
    [[ -f "$transcript_file" ]] || die "no transcript file at ${transcript_file:-<unset>}"

    uuid=$(resolve_ref "$ref")
    item=$(fetch_sweep | jq -er --arg id "$uuid" 'first(.items[] | select(.id == $id)) // empty') \
      || die "no item $uuid in the sweep"
    version=$(jq -er '.version' <<<"$item") || die "item row carries no version"

    transcript=$(cat "$transcript_file")
    [[ -n "$transcript" ]] || die "transcript file is empty"
    grill_id=$(deterministic_id "$uuid/$transcript")

    payload=$(jq -n \
      --arg id "$grill_id" --arg item_id "$uuid" --argjson v "$version" \
      --arg transcript "$transcript" --arg summary "$summary" \
      --arg verdict "$verdict" --arg proposal "$proposal" \
      --arg patch "$applied_patch" --argjson del "$delete_plan" \
      '{id: $id, item_id: $item_id, expected_version: $v,
        transcript: $transcript, summary: $summary, verdict: $verdict,
        model_proposal: $proposal, applied_patch: $patch,
        delete_unticked_plan: $del}')

    request POST /api/grills "$payload"
    case "$STATUS" in
      201) printf '%s\n' "$BODY" ;;
      # Idempotent by the deterministic id: a replayed record is success.
      200) printf '%s\n' "$BODY" ;;
      409) die "POST /api/grills conflicted — the item moved (another writer, or edits made after this script read it); re-read, re-apply your edits if needed, and record again: $BODY" ;;
      *) die "POST /api/grills answered $STATUS: $BODY" ;;
    esac
    ;;

  *)
    cat >&2 <<'USAGE'
usage: grill-record.sh record <ref> --verdict resolved|fog_remains \
         --summary <text> --transcript-file <path> \
         [--proposal <text>] [--applied-patch <text>] [--delete-unticked-plan]
USAGE
    exit 2
    ;;
esac
