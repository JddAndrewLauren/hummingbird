#!/usr/bin/env bash
# Survey + rank helper for the /next-up-hb skill (#116), plus the four verbs
# #10's delegation protocol needs (#115/#291).
#
# Usage: next-up.sh survey [--context X] [--energy low|medium|high]
#                          [--size quick|normal|deep] [--agent]
#                          [--calendar <file>]
#                          [--now-local YYYY-MM-DDTHH:MM] [--now-epoch-ms N]
#        next-up.sh rank                 # a prebuilt envelope on stdin
#        next-up.sh get <ref>            # one item, its steps and its open blockers
#        next-up.sh move <ref> <stage>   # the claim, and the hand-back
#        next-up.sh note <ref> <file>    # the findings
#        next-up.sh unflag-agent <ref>   # what closes the loop
#
# **The survey half is read-only and the delegation half is not**, and the
# split is worth reading rather than assuming. `survey` and `rank` make one
# `GET` and no write; the four verbs below are #10's protocol, which is
# three CAS writes and one read. That protocol is fixed by #10 and this
# script implements it -- it does not decide it.
#
# Credential posture, worth stating where the token is read rather than
# only in a doc: a `device` token is the authority's **only read-capable
# scope**, and it is write-everything. Before the delegation verbs landed
# that meant a read-only script carrying a write credential; now it
# actually uses part of it -- but the observation that matters is
# unchanged, and is why this never goes into GitHub Actions any more than
# `server/scripts/smoke-prod.sh` does.
#
# `survey` and `rank` exist because the skill has two arms (#41):
#   survey -- the interactive arm, which holds the token and fetches;
#   rank   -- the hosted skill-runner arm, which is context-blind: the
#             sweep payload arrives in the `{skill, args}` request from the
#             calling device's mirror, so the runner holds no authority
#             token and makes no HTTP call.
# The delegation verbs are the interactive arm's alone. The runner arm has
# no shell and no credential, so it cannot reach them at all -- which is
# the structural half of "the hosted arm never writes".
#
# Three shapes fall out of the owned API and are worth stating once:
#
# 1. **Every read is the whole sweep.** There is no `GET /api/items/:id`,
#    so `get` fetches the sweep and filters it in jq. Simpler than the
#    GraphQL it replaces, with one bounded payload to filter locally.
# 2. **`HB-<seq>` is a client-side affordance.** No route accepts or
#    resolves it; `seq` is server-minted and appears only in `Item.seq`.
# 3. **A write is CAS, retried exactly once** -- read `version`, `PATCH`
#    with `expected_version`, and on a 409 compare touched fields with the
#    original row. Disjoint changes are resent against `.current.version`;
#    same-field changes stop rather than overwriting them.
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

token() {
  # Preflight: one provisioning line, non-zero exit, no stack trace and no
  # offer to mint one (`linear.sh`'s discipline, kept).
  [[ -f "$TOKEN_PATH" ]] || die "no authority token at $TOKEN_PATH — mint a device-scope token against $API_BASE (POST /api/admin/tokens, ADMIN_SECRET) and save it to that path"
  # The token is read from a file and passed in a header — never on a
  # command line, where `ps` and shell history would both see it.
  tr -d '[:space:]' <"$TOKEN_PATH"
}

# One request. Leaves the body in $BODY and the status in $STATUS and
# asserts neither — a 409 is an outcome for the CAS verbs, not a failure.
request() { # method path [json-body]
  local method=$1 path=$2 data=${3:-} raw auth_token auth_file curl_status
  auth_token=$(token)
  [[ -n "$auth_token" ]] || die "empty authority token at $TOKEN_PATH — mint a device-scope token against $API_BASE (POST /api/admin/tokens, ADMIN_SECRET) and save it to that path"
  auth_file=$(mktemp)
  chmod 600 "$auth_file"
  printf 'Authorization: Bearer %s\n' "$auth_token" >"$auth_file"
  local args=(-sS -w '\n%{http_code}' --connect-timeout 10 --max-time 30
              -X "$method" -H "@$auth_file" "$API_BASE$path")
  [[ -n "$data" ]] && args+=(-H 'Content-Type: application/json' -d "$data")
  if raw=$(curl "${args[@]}"); then
    :
  else
    curl_status=$?
    rm -f "$auth_file"
    return "$curl_status"
  fi
  rm -f "$auth_file"
  # Split on the last newline (BSD head rejects `head -n -1`).
  BODY=${raw%$'\n'*}
  STATUS=${raw##*$'\n'}
}

# Fetched at most once per run, however many times it is read.
#
# The cache is a **file**, not a variable, and that is not a style choice:
# `resolve_ref` is called as `$(…)`, and a variable assigned inside a
# command substitution never reaches the parent — so a variable cache would
# silently re-fetch on every read and let one run reason over two different
# sweeps. The path is minted before any subshell exists, so every subshell
# writes and reads the same file, and the `EXIT` trap does not fire in a
# command substitution.
SWEEP_CACHE=$(mktemp)
trap 'rm -f "$SWEEP_CACHE"' EXIT

fetch_sweep() {
  if [[ -s "$SWEEP_CACHE" ]]; then
    cat "$SWEEP_CACHE"
    return
  fi

  # A fixture short-circuits the fetch entirely, so the survey arm is
  # exercisable with no credential at all. It deliberately does NOT cover
  # the delegation verbs: those write, and a fixture that let a write
  # "succeed" against a file would be a lie about the only thing worth
  # verifying.
  if [[ -n "${HB_SWEEP_FIXTURE:-}" ]]; then
    [[ -f "$HB_SWEEP_FIXTURE" ]] || die "no sweep fixture at $HB_SWEEP_FIXTURE"
    cat "$HB_SWEEP_FIXTURE" >"$SWEEP_CACHE"
    cat "$SWEEP_CACHE"
    return
  fi

  request GET /api/sweep
  [[ "$STATUS" == "200" ]] || die "GET $API_BASE/api/sweep answered $STATUS"
  # The SPA shell answers 200 text/html on an unmatched path, so a 200 is
  # not on its own proof the API was reached (smoke-prod.sh's own check).
  jq -e 'has("items")' <<<"$BODY" >/dev/null 2>&1 || die "GET $API_BASE/api/sweep did not answer a sweep payload"
  printf '%s' "$BODY" >"$SWEEP_CACHE"
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

item_row() { # uuid -> the item row
  fetch_sweep | jq -er --arg id "$1" 'first(.items[] | select(.id == $id)) // empty' \
    || die "no item $1 in the sweep"
}

# A CAS patch with exactly one bounded retry. `$3` is the complete row read
# before the first attempt, so a conflict can distinguish disjoint changes
# from a concurrent edit to one of the fields this operation touches.
cas_patch() { # path fields-json base-row
  local path=$1 fields=$2 base=$3 attempt version current conflicts
  version=$(jq -er '.version' <<<"$base") \
    || die "PATCH $path cannot read the original row version: $base"
  for attempt in 1 2; do
    request PATCH "$path" "$(jq -c --argjson v "$version" '. + {expected_version: $v}' <<<"$fields")"
    case "$STATUS" in
      200) printf '%s\n' "$BODY"; return 0 ;;
      409)
        [[ "$attempt" == 1 ]] || die "PATCH $path still conflicting after one retry — another writer is moving the same row; report where the protocol stopped rather than grinding: $BODY"
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

cmd="${1:-}"
shift || true

case "$cmd" in
  survey)
    context="" energy="" size="" calendar_file="" agent_only=false
    now_local="$(date +%Y-%m-%dT%H:%M)"
    now_epoch_ms="$(( $(date +%s) * 1000 ))"
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --context)      context="$2"; shift 2 ;;
        --energy)       energy="$2"; shift 2 ;;
        --size)         size="$2"; shift 2 ;;
        --agent)        agent_only=true; shift ;;
        --calendar)     calendar_file="$2"; shift 2 ;;
        --now-local)    now_local="$2"; shift 2 ;;
        --now-epoch-ms) now_epoch_ms="$2"; shift 2 ;;
        *) die "unknown option $1" ;;
      esac
    done
    [[ -z "$energy" || "$energy" =~ ^(low|medium|high)$ ]] || die "--energy must be low, medium or high"
    [[ -z "$size" || "$size" =~ ^(quick|normal|deep)$ ]] || die "--size must be quick, normal or deep"
    [[ -z "$calendar_file" || -f "$calendar_file" ]] || die "no calendar context file at $calendar_file"

    sweep=$(fetch_sweep)
    jq -n \
      --argjson sweep "$sweep" \
      --arg context "$context" --arg energy "$energy" --arg size "$size" \
      --arg nowLocal "$now_local" --argjson nowEpochMs "$now_epoch_ms" \
      --argjson agentOnly "$agent_only" \
      --argjson calendar "$([[ -n "$calendar_file" ]] && cat "$calendar_file" || echo null)" \
      '{
         sweep: $sweep,
         axes: ({context: $context, energy: $energy, size: $size}
                | with_entries(select(.value != ""))),
         now: {local: $nowLocal, epoch_ms: $nowEpochMs},
         agent_only: $agentOnly
       }
       | if $calendar == null then . else . + {calendar: $calendar} end' \
      | rank_envelope
    ;;

  rank)
    rank_envelope
    ;;

  # ----------------------------------------- #10's delegation protocol
  #
  # Four verbs, and the protocol they serve is fixed by #10: claim on
  # start, findings then Ready then clear on finish, never Done. This
  # script implements it and SKILL.md states it; neither re-decides it.

  get)
    # The read the branch opens with, so a directly-named item can be
    # checked before anything is claimed: does it carry the axis, is
    # anything blocking it, is it already shut.
    ref="${1:?usage: next-up.sh get <ref>}"
    uuid=$(resolve_ref "$ref")
    fetch_sweep | jq --arg id "$uuid" '
      . as $s
      | first($s.items[] | select(.id == $id)) as $item
      | {
          item: $item,
          steps: ([$s.steps[] | select(.item_id == $id and .deleted_at == null)]
                  | sort_by(.position, .id)),
          # An open blocker is a live edge whose blocker is not itself shut.
          # A blocker absent from the sweep counts as blocking: a payload
          # that does not mention a row is not evidence the row is finished.
          blockers: [$s.blocked_by[]
                     | select(.item_id == $id and .removed_at == null)
                     | .blocker_id
                     | . as $b
                     | first($s.items[] | select(.id == $b)) as $row
                     | select($row == null
                              or ($row.stage != "done" and $row.archived_at == null))
                     | {id: $b, seq: ($row.seq // null), title: ($row.title // null),
                        stage: ($row.stage // null)}]
        }'
    ;;

  move)
    ref="${1:?usage: next-up.sh move <ref> <stage>}"
    stage="${2:?usage: next-up.sh move <ref> <stage>}"
    # The owned schema's own spellings, resolved by name before the seam so
    # a typo fails here rather than reaching the authority. `done` is
    # accepted by the vocabulary and refused by the protocol: an agent
    # chore *advances* a chore, it does not complete it (#10), and the
    # human takes the decision.
    [[ "$stage" =~ ^(triage|grilling|ready|in_progress|blocked)$ ]] || {
      if [[ "$stage" == "done" ]]; then
        die "the delegation protocol never moves an item to done (#10) — an agent chore advances a chore, it does not finish it; hand it back with 'move <ref> ready'"
      fi
      die "--stage must be one of triage, grilling, ready, in_progress, blocked"
    }
    uuid=$(resolve_ref "$ref")
    row=$(item_row "$uuid")
    if [[ "$(jq -r '.stage' <<<"$row")" == "$stage" ]]; then
      printf '%s\n' "$row"
      exit 0
    fi
    cas_patch "/api/items/$uuid" "$(jq -n --arg s "$stage" '{stage: $s}')" "$row"
    ;;

  note)
    ref="${1:?usage: next-up.sh note <ref> <file>}"
    file="${2:?usage: next-up.sh note <ref> <file>}"
    [[ -f "$file" ]] || die "no findings file at $file"
    uuid=$(resolve_ref "$ref")
    row=$(item_row "$uuid")

    # The findings lane, and it is an acknowledged stopgap: the owned
    # schema has no comments table, so this appends to `description` under
    # a delimited section (ADR-0009's 2026-08-11 amendment records why, and
    # what would flip it to a real `notes` table).
    #
    # A re-run REPLACES the section rather than appending a second one —
    # which is what keeps a resumed half-finished finish from leaving two
    # near-identical findings blocks, the same duplication #10 worries
    # about. Everything outside the markers is left exactly as it was.
    # Spliced by substring index, never by regex: the markers and the
    # surrounding description are arbitrary prose, and a regex here would
    # have to escape both. `index` answers the only question there is.
    updated=$(jq -rn --arg desc "$(jq -r '.description // ""' <<<"$row")" \
      --rawfile findings "$file" '
        ($findings | sub("\\s+$"; "")) as $body
        | "<!-- agent-findings -->" as $start
        | "<!-- /agent-findings -->" as $end
        | ($start + "\n## Agent findings\n\n" + $body + "\n" + $end) as $section
        | ($desc | index($start)) as $i
        | ($desc | index($end)) as $j
        | if $i != null and $j != null and $j > $i
          then $desc[0:$i] + $section + $desc[($j + ($end | length)):]
          elif ($desc | length) == 0
          then $section
          else $desc + "\n\n" + $section
          end')

    cas_patch "/api/items/$uuid" "$(jq -n --arg d "$updated" '{description: $d}')" "$row"
    ;;

  unflag-agent)
    # What closes the loop. `agent` means there is agent work *left* here,
    # not that an agent touched this once — leave it set and the next
    # survey re-offers the hand-off and the agent redoes its own research.
    # Idempotent: clearing an already-clear axis is success, so a re-run of
    # a half-finished finish is safe.
    ref="${1:?usage: next-up.sh unflag-agent <ref>}"
    uuid=$(resolve_ref "$ref")
    row=$(item_row "$uuid")
    if [[ "$(jq -r '.agent' <<<"$row")" != "true" ]]; then
      printf '%s\n' "$row"
      exit 0
    fi
    cas_patch "/api/items/$uuid" '{"agent": false}' "$row"
    ;;

  *)
    cat >&2 <<'USAGE'
usage: next-up.sh survey [--context X] [--energy low|medium|high]
                         [--size quick|normal|deep] [--agent] [--calendar <file>]
       next-up.sh rank
       next-up.sh get <ref>
       next-up.sh move <ref> <triage|grilling|ready|in_progress|blocked>
       next-up.sh note <ref> <file>
       next-up.sh unflag-agent <ref>
USAGE
    exit 2
    ;;
esac
