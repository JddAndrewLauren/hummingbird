#!/usr/bin/env bash
# Authority helper for the OpenClaw `hummingbird-tasks` skill (ADR-0029).
#
# Usage: hb-tasks.sh sweep [--json]          # live items, compact render (or raw sweep)
#        hb-tasks.sh add <title> [<field flags>]
#        hb-tasks.sh edit <ref> [<field flags>]
#        hb-tasks.sh done <ref>              # stage -> done, CAS
#
# Field flags (shared by add and edit; edit also takes --title):
#        --title <t> --notes <t> --stage <s> --priority <0-4>
#        --deadline <YYYY-MM-DD[THH:MM]> --scheduled <YYYY-MM-DD>
#        --size quick|normal|deep --energy low|medium|high --context <t>
#
# Scope guard, enforced here and not only in prose: this script writes
# `items` rows and nothing else — `POST /api/items` and
# `PATCH /api/items/:id` are the only write paths. There is no verb that
# touches a step, a project, a route or a grill, so a slip in SKILL.md
# cannot become a write the skill was never allowed to make. Step writes
# belong to the ported microtask skill's own `hb.sh`; grill records to
# grill-me's `grill-record.sh` (each self-contained, per this repo's
# per-skill-script pattern — the duplication with
# `.claude/skills/microtask/scripts/hb.sh` is deliberate and inherited
# from it, see that file's header).
#
# Shapes inherited from the owned API (stated once in hb.sh, restated in
# brief): every read is the whole `GET /api/sweep` (no by-id item read);
# `HB-<seq>` is a client-side affordance resolved off the fetched sweep; a
# write is CAS by `expected_version` with exactly one bounded 409 retry.
#
# `add` deliberately sends no `source`: an OpenClaw add is an operator
# capture, not an evaluated stream (ADR-0011's per-source table governs
# `items.source` values), so it lands exactly as a hand capture does —
# stage defaulting to Triage server-side. The id is a random v4 (`uuidgen`,
# or the kernel's uuid source where that is not installed — see `mint_id`),
# not a deterministic one: two same-titled adds are two items.
set -euo pipefail

command -v jq >/dev/null || { echo "hb-tasks.sh: jq is required" >&2; exit 1; }
command -v curl >/dev/null || { echo "hb-tasks.sh: curl is required" >&2; exit 1; }

API_BASE="${HB_API_BASE:-https://hb.twinion.net}"
TOKEN_PATH="${HB_API_TOKEN_PATH:-$HOME/.config/hummingbird/api-token}"

die() { echo "hb-tasks.sh: $*" >&2; exit 1; }

token() {
  # Read from a file and passed in a header — never on a command line,
  # where `ps` and shell history would both see it.
  [[ -f "$TOKEN_PATH" ]] || die "no authority token at $TOKEN_PATH — mint a device-scope token against $API_BASE (POST /api/admin/tokens, ADMIN_SECRET) and save it to that path"
  tr -d '[:space:]' <"$TOKEN_PATH"
}

# One request. Leaves the body in $BODY and the status in $STATUS, and does
# NOT assert either — every caller here has its own idea of which statuses
# are success (a 409 is an outcome, not a failure).
request() { # method path [json-body]
  local method=$1 path=$2 data=${3:-} raw auth_token curl_status
  auth_token=$(token)
  [[ -n "$auth_token" ]] || die "empty authority token at $TOKEN_PATH — mint a device-scope token against $API_BASE (POST /api/admin/tokens, ADMIN_SECRET) and save it to that path"
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
  # Split on the last newline (BSD head rejects `head -n -1`).
  BODY=${raw%$'\n'*}
  STATUS=${raw##*$'\n'}
}

# Fetched at most once per run. A file, not a variable: helpers run inside
# `$(…)` subshells, where a variable cache never reaches the parent (see
# hb.sh's header for the full trap).
SWEEP_CACHE=$(mktemp)
trap 'rm -f "$SWEEP_CACHE"' EXIT

fetch_sweep() {
  if [[ ! -s "$SWEEP_CACHE" ]]; then
    request GET /api/sweep
    [[ "$STATUS" == "200" ]] || die "GET /api/sweep answered $STATUS: $BODY"
    # The SPA shell answers `200 text/html` on an unmatched path, so a 200
    # is not on its own proof the API was reached.
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

item_row() { # item-uuid -> the row
  fetch_sweep | jq -er --arg id "$1" 'first(.items[] | select(.id == $id)) // empty' \
    || die "no item $1 in the sweep"
}

# A CAS patch with exactly one bounded retry — hb.sh's recipe verbatim.
cas_patch() { # path fields-json base-row
  local path=$1 fields=$2 base=$3 attempt version current conflicts
  version=$(jq -er '.version' <<<"$base") \
    || die "PATCH $path cannot read the original row version: $base"
  for attempt in 1 2; do
    request PATCH "$path" "$(jq -c --argjson v "$version" '. + {expected_version: $v}' <<<"$fields")"
    case "$STATUS" in
      200) printf '%s\n' "$BODY"; return 0 ;;
      409)
        [[ "$attempt" == 1 ]] || die "PATCH $path still conflicting after one retry — another writer is moving the same row; re-read and decide rather than grinding: $BODY"
        current=$(jq -er '.current' <<<"$BODY") \
          || die "PATCH $path answered 409 without a current entity: $BODY"
        conflicts=$(jq -r --argjson base "$base" --argjson current "$current" '
          [to_entries[]
           | select($current[.key] != $base[.key])
           | select($current[.key] != .value)
           | .key]
          | join(", ")
        ' <<<"$fields") \
          || die "PATCH $path could not compare the conflicting entity: $BODY"
        [[ -z "$conflicts" ]] || die "PATCH $path has a same-field conflict on $conflicts — the current value changed; re-read and decide: $BODY"
        if jq -e --argjson current "$current" \
          'all(to_entries[]; $current[.key] == .value)' <<<"$fields" >/dev/null; then
          printf '%s\n' "$current"
          return 0
        fi
        version=$(jq -er '.version' <<<"$current") \
          || die "PATCH $path answered 409 without a current entity version: $BODY"
        ;;
      *) die "PATCH $path answered $STATUS: $BODY" ;;
    esac
  done
}

mint_id() { # -> lowercase uuid v4
  # bash + curl + jq is the dependency floor (#609 decision 3), so uuidgen
  # is not assumed: it ships in `uuid-runtime`, which a fresh WSL/Ubuntu
  # gateway does not have. The kernel's own v4 source covers that case
  # without an operator install step; the die stays as the last resort.
  if command -v uuidgen >/dev/null; then
    uuidgen | tr '[:upper:]' '[:lower:]'
  elif [[ -r /proc/sys/kernel/random/uuid ]]; then
    tr '[:upper:]' '[:lower:]' </proc/sys/kernel/random/uuid
  else
    die "no uuid source: install uuid-runtime (uuidgen) to mint an item id"
  fi
}

STAGES="triage grilling ready in_progress blocked done"
# `quick normal deep` and `low medium high` mirror domain::Size and
# domain::Energy (`server/domain/src/item.rs`). The pre-schema-7 spelling
# `short` is deliberately NOT accepted: it survives as an inbound alias
# only, for writes minted before the deploy and drained after it, and
# `Size::as_str` never emits it — a fresh write from here has no business
# reintroducing the old word.
SIZES="quick normal deep"
ENERGIES="low medium high"

valid_stage() {
  local s
  for s in $STAGES; do [[ "$1" == "$s" ]] && return 0; done
  return 1
}

valid_size() {
  local s
  for s in $SIZES; do [[ "$1" == "$s" ]] && return 0; done
  return 1
}

valid_energy() {
  local s
  for s in $ENERGIES; do [[ "$1" == "$s" ]] && return 0; done
  return 1
}

# `scheduled_date` is the one field here the authority does NOT validate —
# `handlers/items.rs` gates `deadline` and nothing else — while every client
# consumer does: `decisions::urgency::is_valid_scheduled_date` is whole-day
# and carries a test refusing `2026-08-30T09:30`. A datetime written from
# here would persist and then be ignored downstream, so this is its only
# gate. `--deadline` needs none: the authority answers a 400 that says so.
valid_scheduled() { # whole-day YYYY-MM-DD
  [[ "$1" =~ ^[0-9]{4}-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])$ ]]
}

# Options shared by add and edit: each sets a key in $FIELDS (a jq object
# accumulated as JSON text). `--notes` maps onto the wire's `description`.
FIELDS='{}'

set_field() { # key json-encoded-value
  FIELDS=$(jq -c --arg k "$1" --argjson v "$2" '. + {($k): $v}' <<<"$FIELDS")
}

parse_field_flags() { # flag args...; echoes count consumed per loop via globals
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --title)     [[ -n "${2:-}" ]] || die "--title needs a value";    set_field title "$(jq -Rn --arg v "$2" '$v')"; shift 2 ;;
      --notes)     [[ -n "${2:-}" ]] || die "--notes needs a value";    set_field description "$(jq -Rn --arg v "$2" '$v')"; shift 2 ;;
      --stage)     valid_stage "${2:-}" || die "--stage must be one of: $STAGES"; set_field stage "\"$2\""; shift 2 ;;
      --priority)  [[ "${2:-}" =~ ^[0-4]$ ]] || die "--priority must be 0-4"; set_field priority "$2"; shift 2 ;;
      --deadline)  [[ -n "${2:-}" ]] || die "--deadline needs a value";  set_field deadline "$(jq -Rn --arg v "$2" '$v')"; shift 2 ;;
      --scheduled) valid_scheduled "${2:-}" || die "--scheduled must be a whole-day YYYY-MM-DD (a time of day is accepted by the authority and then ignored by every client)"; set_field scheduled_date "$(jq -Rn --arg v "$2" '$v')"; shift 2 ;;
      # The three axes a grill resolves. Without these, an accepted
      # proposal that sizes an item has no way to land (grill-me's
      # SKILL.md names them in its patch, and this is the only writer).
      --size)      valid_size "${2:-}"   || die "--size must be one of: $SIZES";     set_field size "\"$2\""; shift 2 ;;
      --energy)    valid_energy "${2:-}" || die "--energy must be one of: $ENERGIES"; set_field energy "\"$2\""; shift 2 ;;
      # `context` is a free string on the wire, not an enum — the GTD
      # @-context vocabulary is the operator's, not the schema's.
      --context)   [[ -n "${2:-}" ]] || die "--context needs a value";   set_field context "$(jq -Rn --arg v "$2" '$v')"; shift 2 ;;
      *) die "unknown flag $1" ;;
    esac
  done
}

# One line per item, the render the agent reads as its working context.
render_line='def pri: if .priority > 0 then " p\(.priority)" else "" end;
  def due: if .deadline then " due \(.deadline)" else "" end;
  def sched: if .scheduled_date then " sched \(.scheduled_date)" else "" end;
  "HB-\(.seq)\t[\(.stage)]\t\(.title)\(pri)\(due)\(sched)"'

cmd="${1:-}"
shift || true

case "$cmd" in
  sweep)
    if [[ "${1:-}" == "--json" ]]; then
      fetch_sweep | jq .
      exit 0
    fi
    # Live working set: not archived, not done, ordered by stage urgency.
    fetch_sweep | jq -r "
      def stage_rank: {in_progress: 0, ready: 1, blocked: 2, grilling: 3, triage: 4}[.stage] // 5;
      [.items[] | select(.archived_at == null and .stage != \"done\")]
      | sort_by([(. | stage_rank), -(.priority), .seq])
      | .[] | $render_line"
    ;;

  add)
    title="${1:?usage: hb-tasks.sh add <title> [field flags — see hb-tasks.sh with no args]}"
    shift
    [[ -n "$title" ]] || die "title must be non-empty"
    parse_field_flags "$@"
    payload=$(jq -c --arg id "$(mint_id)" --arg title "$title" \
      '. + {id: $id, title: $title}' <<<"$FIELDS")
    request POST /api/items "$payload"
    case "$STATUS" in
      201) jq -r "$render_line" <<<"$BODY" ;;
      # Idempotent by client id; a replayed random uuid cannot collide in
      # practice, but the route's contract is stated anyway.
      200) jq -r "$render_line" <<<"$BODY" ;;
      *) die "POST /api/items answered $STATUS: $BODY" ;;
    esac
    ;;

  edit)
    ref="${1:?usage: hb-tasks.sh edit <ref> [field flags — see hb-tasks.sh with no args]}"
    shift
    parse_field_flags "$@"
    [[ "$FIELDS" != '{}' ]] || die "edit needs at least one field flag"
    uuid=$(resolve_ref "$ref")
    row=$(item_row "$uuid")
    cas_patch "/api/items/$uuid" "$FIELDS" "$row" | jq -r "$render_line"
    ;;

  done)
    ref="${1:?usage: hb-tasks.sh done <ref>}"
    uuid=$(resolve_ref "$ref")
    row=$(item_row "$uuid")
    if [[ "$(jq -r '.stage' <<<"$row")" == "done" ]]; then
      # Already done, here or in a client. Not an error: the surfaces agree.
      jq -r "$render_line" <<<"$row"
      exit 0
    fi
    cas_patch "/api/items/$uuid" '{"stage": "done"}' "$row" | jq -r "$render_line"
    ;;

  *)
    cat >&2 <<'USAGE'
usage: hb-tasks.sh sweep [--json]
       hb-tasks.sh add <title> [field flags]
       hb-tasks.sh edit <ref> [field flags]
       hb-tasks.sh done <ref>

field flags (add and edit both; --title is only useful on edit):
       --title <t>          --notes <t>              --stage triage|grilling|ready|in_progress|blocked|done
       --priority <0-4>     --deadline <YYYY-MM-DD[THH:MM]>          --scheduled <YYYY-MM-DD>
       --size quick|normal|deep                      --energy low|medium|high
       --context <t>
USAGE
    exit 2
    ;;
esac
