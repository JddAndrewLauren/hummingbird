#!/usr/bin/env bash
# Authority helper for the /microtask skill (#115). Replaces `linear.sh`.
#
# Usage: hb.sh get <ref>                    # the item and its live steps
#        hb.sh steps <ref>                  # just the live steps, in position order
#        hb.sh add-step  <ref> <body>       # one step, appended
#        hb.sh add-steps <ref> <file>       # one step per non-blank line, appended
#        hb.sh tick <step-id>               # {done: true}, CAS
#        hb.sh drop-step <step-id>          # {deleted_at: now}, CAS — flagged, never erased
#
# Scope guard, enforced here and not only in prose: this script writes
# `steps` rows and nothing else. There is no verb that touches an item, a
# project, a route or a label, so a slip in SKILL.md cannot become a write
# the skill was never allowed to make.
#
# ---------------------------------------------------------------------------
# Three shapes fall out of the owned API and are worth stating once, because
# each one deletes machinery the Linear-era script needed.
#
# 1. **Every read is the whole sweep.** There is no `GET /api/items/:id` and
#    no `GET /api/steps` — `GET /api/sweep` is the only read of domain data,
#    so each read verb below fetches once and then filters in jq. Simpler
#    than the GraphQL it replaces, and it is why the old REFERENCE.md's
#    complexity-cap findings are not merely obsolete but meaningless.
#
# 2. **`HB-<seq>` is a client-side affordance.** No route accepts or
#    resolves it: `seq` is server-minted at create and appears only in
#    `Item.seq`. `resolve_ref` maps `HB-42` onto its uuid off the sweep it
#    has already fetched, and passes a uuid through untouched.
#
# 3. **A write is CAS, and a 409 is retried exactly once.** Read `version`,
#    `PATCH` with `expected_version`, and on a conflict re-read
#    `.current.version` from the 409's own body and reissue. Once, bounded —
#    the same shape and the same reasoning as `write/adapter.rs`'s
#    `MAX_ATTEMPTS`: a second disjoint conflict is repeated churn, not a
#    collision to keep grinding against.
#
# The Linear-era `<!-- microtask:start -->` markers, the read-modify-write
# merge around a human's edits, and the `- [x]`/`- [X]` normalisation are all
# **deleted rather than ported**. All three existed only because the body was
# one opaque string two parties wrote to. `steps` is a table with a `done`
# column and a per-row `version`; ticking one is the scalar CAS write
# `server/domain/src/step.rs` names as "the operation whose impossibility
# under Linear triggered ADR-0008".
#
# This script is self-contained, and the duplication with the other skills'
# `hb.sh` is deliberate: `runner/Dockerfile` bakes skill directories in
# individually, so a shared helper living outside this tree would simply not
# ship in the image.
set -euo pipefail

command -v jq >/dev/null || { echo "hb.sh: jq is required" >&2; exit 1; }
command -v curl >/dev/null || { echo "hb.sh: curl is required" >&2; exit 1; }

API_BASE="${HB_API_BASE:-https://hb.twinion.net}"
TOKEN_PATH="${HB_API_TOKEN_PATH:-$HOME/.config/hummingbird/api-token}"

# Frozen, and disjoint from every other id space by the same hash-domain
# separation `client/core/src/sync/write/id.rs` and `sweep.py` rely on.
# Changing it re-mints every step this skill has ever written.
ID_NAMESPACE="hummingbird-skill/microtask/v1"

die() { echo "hb.sh: $*" >&2; exit 1; }

token() {
  # One provisioning line, non-zero exit, no stack trace and no offer to
  # mint one. The token is read from a file and passed in a header — never
  # on a command line, where `ps` and shell history would both see it.
  [[ -f "$TOKEN_PATH" ]] || die "no authority token at $TOKEN_PATH — mint a device-scope token against $API_BASE (POST /api/admin/tokens, ADMIN_SECRET) and save it to that path"
  tr -d '[:space:]' <"$TOKEN_PATH"
}

# One request. Leaves the body in $BODY and the status in $STATUS, and does
# NOT assert either — every caller here has its own idea of which statuses
# are success (a 409 is an outcome, not a failure).
request() { # method path [json-body]
  local method=$1 path=$2 data=${3:-} raw
  local args=(-sS -w '\n%{http_code}' --connect-timeout 10 --max-time 30
              -X "$method" -H "Authorization: Bearer $(token)" "$API_BASE$path")
  [[ -n "$data" ]] && args+=(-H 'Content-Type: application/json' -d "$data")
  raw=$(curl "${args[@]}")
  # Split on the last newline (BSD head rejects `head -n -1`).
  BODY=${raw%$'\n'*}
  STATUS=${raw##*$'\n'}
}

sha256_hex() { # stdin -> 64 hex chars
  if command -v sha256sum >/dev/null; then sha256sum | cut -d' ' -f1
  else shasum -a 256 | cut -d' ' -f1; fi
}

# `sha256(namespace + seed)`, first 16 bytes, version/variant nibbles forced
# into UUID v4 shape — `deterministic_id`'s recipe, in bash. A create is
# idempotent by its client-supplied id, so an interrupted batch replayed
# from the same inputs lands on "already exists" instead of minting a
# second copy of every step.
deterministic_id() { # seed -> uuid
  local hex b6 b8
  hex=$(printf '%s%s' "$ID_NAMESPACE" "$1" | sha256_hex)
  hex=${hex:0:32}
  b6=$(( 0x${hex:12:2} & 0x0F | 0x40 ))
  b8=$(( 0x${hex:16:2} & 0x3F | 0x80 ))
  hex="${hex:0:12}$(printf '%02x' "$b6")${hex:14:2}$(printf '%02x' "$b8")${hex:18:14}"
  printf '%s-%s-%s-%s-%s' "${hex:0:8}" "${hex:8:4}" "${hex:12:4}" "${hex:16:4}" "${hex:20:12}"
}

# Fetched at most once per run, however many times it is read.
#
# The cache is a **file**, not a variable, and that is not a style choice:
# `resolve_ref` and friends are called as `$(…)`, and a variable assigned
# inside a command substitution never reaches the parent — so a variable
# cache would silently re-fetch on every read and let one run reason over
# two different sweeps. The path is minted before any subshell exists, so
# every subshell writes and reads the same file. The `EXIT` trap does not
# fire in a command substitution, so the cache survives to the real exit.
SWEEP_CACHE=$(mktemp)
trap 'rm -f "$SWEEP_CACHE"' EXIT

sweep() {
  if [[ ! -s "$SWEEP_CACHE" ]]; then
    request GET /api/sweep
    [[ "$STATUS" == "200" ]] || die "GET /api/sweep answered $STATUS: $BODY"
    # The SPA shell answers `200 text/html` on an unmatched path, so a 200
    # is not on its own proof the API was reached (smoke-prod.sh's check).
    jq -e 'has("items")' <<<"$BODY" >/dev/null 2>&1 || die "GET /api/sweep did not answer a sweep payload"
    printf '%s' "$BODY" >"$SWEEP_CACHE"
  fi
  cat "$SWEEP_CACHE"
}

resolve_ref() { # HB-42 | uuid -> uuid
  local ref=$1
  if [[ "$ref" =~ ^[Hh][Bb]-([0-9]+)$ ]]; then
    sweep | jq -er --argjson seq "${BASH_REMATCH[1]}" \
      'first(.items[] | select(.seq == $seq) | .id) // empty' \
      || die "no item with seq ${BASH_REMATCH[1]} in the sweep"
  else
    sweep | jq -er --arg id "$ref" 'first(.items[] | select(.id == $id) | .id) // empty' \
      || die "no item $ref in the sweep"
  fi
}

# A CAS patch with exactly one bounded retry. `$1` is the path, `$2` a jq
# object of the fields to set (without `expected_version`), `$3` the version
# to try first.
cas_patch() { # path fields-json expected-version
  local path=$1 fields=$2 version=$3 attempt
  for attempt in 1 2; do
    request PATCH "$path" "$(jq -c --argjson v "$version" '. + {expected_version: $v}' <<<"$fields")"
    case "$STATUS" in
      200) printf '%s\n' "$BODY"; return 0 ;;
      409)
        [[ "$attempt" == 1 ]] || die "PATCH $path still conflicting after one retry — another writer is moving the same row; re-read and decide rather than grinding: $BODY"
        version=$(jq -er '.current.version' <<<"$BODY") \
          || die "PATCH $path answered 409 without a current entity: $BODY"
        ;;
      *) die "PATCH $path answered $STATUS: $BODY" ;;
    esac
  done
}

now_ms() { printf '%s' "$(( $(date +%s) * 1000 ))"; }

live_steps() { # item-uuid -> the item's live steps, position order
  sweep | jq --arg item "$1" \
    '[.steps[] | select(.item_id == $item and .deleted_at == null)]
     | sort_by(.position, .id)'
}

step_row() { # step-id -> the row, or a named failure
  sweep | jq -er --arg id "$1" 'first(.steps[] | select(.id == $id)) // empty' \
    || die "no step $1 in the sweep"
}

cmd="${1:-}"
shift || true

case "$cmd" in
  get)
    ref="${1:?usage: hb.sh get <ref>}"
    uuid=$(resolve_ref "$ref")
    sweep | jq --arg item "$uuid" \
      '{item: first(.items[] | select(.id == $item)),
        steps: ([.steps[] | select(.item_id == $item and .deleted_at == null)]
                | sort_by(.position, .id))}'
    ;;

  steps)
    ref="${1:?usage: hb.sh steps <ref>}"
    # Assigned first, then used — never `live_steps "$(resolve_ref "$ref")"`.
    # A `die` inside `$(…)` exits only that subshell, and `set -e` ignores a
    # failed substitution sitting in an *argument* position: the inline form
    # calls `live_steps ""` and prints a cheerful `[]` for an item that does
    # not exist. An assignment's status is the substitution's, so `set -e`
    # sees it. (Caught by `tests/test_hb_helper.py`, not by reading.)
    uuid=$(resolve_ref "$ref")
    live_steps "$uuid"
    ;;

  add-step|add-steps)
    ref="${1:?usage: hb.sh $cmd <ref> <body-or-file>}"
    arg="${2:?usage: hb.sh $cmd <ref> <body-or-file>}"
    uuid=$(resolve_ref "$ref")

    bodies=()
    if [[ "$cmd" == "add-steps" ]]; then
      [[ -f "$arg" ]] || die "no step file at $arg"
      while IFS= read -r line; do
        [[ -n "${line//[[:space:]]/}" ]] && bodies+=("$line")
      done <"$arg"
      [[ ${#bodies[@]} -gt 0 ]] || die "$arg has no non-blank lines"
    else
      bodies=("$arg")
    fi

    # Appended after whatever is already there. Positions are read once, so
    # a batch numbers itself contiguously rather than re-reading a sweep
    # this run has already cached.
    next=$(live_steps "$uuid" | jq '(map(.position) | max // 0) + 1')
    for body in "${bodies[@]}"; do
      # The seed carries the item and the body but NOT the position, so
      # re-running a checklist whose steps shifted down by one does not
      # mint a second copy of every step.
      id=$(deterministic_id "$uuid/$body")
      request POST /api/steps "$(jq -n --arg id "$id" --arg item "$uuid" \
        --arg body "$body" --argjson pos "$next" \
        '{id: $id, item_id: $item, body: $body, position: $pos}')"
      case "$STATUS" in
        201) printf '%s\n' "$BODY" ;;
        # Idempotent by client id: a replay is success, not a duplicate.
        200) printf '%s\n' "$BODY" ;;
        *) die "POST /api/steps answered $STATUS: $BODY" ;;
      esac
      next=$(( next + 1 ))
    done
    ;;

  tick)
    id="${1:?usage: hb.sh tick <step-id>}"
    row=$(step_row "$id")
    if [[ "$(jq -r '.done' <<<"$row")" == "true" ]]; then
      # Already ticked, by this skill or in the client. Not an error: the
      # walk-through's whole point is that the two surfaces agree.
      printf '%s\n' "$row"
      exit 0
    fi
    cas_patch "/api/steps/$id" '{"done": true}' "$(jq -r '.version' <<<"$row")"
    ;;

  drop-step)
    id="${1:?usage: hb.sh drop-step <step-id>}"
    row=$(step_row "$id")
    if [[ "$(jq -r '.deleted_at' <<<"$row")" != "null" ]]; then
      printf '%s\n' "$row"
      exit 0
    fi
    cas_patch "/api/steps/$id" "$(jq -n --argjson t "$(now_ms)" '{deleted_at: $t}')" \
      "$(jq -r '.version' <<<"$row")"
    ;;

  *)
    cat >&2 <<'USAGE'
usage: hb.sh get <ref>
       hb.sh steps <ref>
       hb.sh add-step  <ref> <body>
       hb.sh add-steps <ref> <file>
       hb.sh tick <step-id>
       hb.sh drop-step <step-id>
USAGE
    exit 2
    ;;
esac
