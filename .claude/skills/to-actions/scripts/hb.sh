#!/usr/bin/env bash
# Authority helper for the /to-actions skill (#115). This skill had no
# script at all under Linear — it hand-wrote GraphQL, the odd one out of the
# three — so this is its first.
#
# Usage: hb.sh project-find <name>                     # the project, its route, fog and actions
#        hb.sh project-create <name>                   # + its route row, created for you
#        hb.sh route-set <project-ref> [--destination <file>] [--notes <file>]
#        hb.sh fog-add <project-ref> <question>
#        hb.sh fog-resolve <fog-id>
#        hb.sh mint <manifest-file>                    # the one confirmed batch
#        hb.sh block <ref> <blocker-ref>               # <ref> is blocked by <blocker-ref>
#        hb.sh archive <ref>                            # cancel by soft archive
#
# ---------------------------------------------------------------------------
# What the owned schema changed, and what that deletes from the old recipe.
#
# **The Route is a record, not a markdown blob.** `## Destination` is
# `routes.destination`, `## Notes` is `routes.notes`, `## Fog` is `fog` rows
# with a `resolved_at`, and `## Actions` is items carrying `project_id`
# ordered by `project_pos`. Routes being first-class is one of the two
# things ADR-0008 was triggered by, so the four-section template is gone
# rather than reimplemented over a description field.
#
# **Creating a project creates its route.** The 1:1 invariant is structural
# and there is deliberately no `POST /api/routes`; `route-set` patches the
# row that already exists.
#
# **`blocked_by` is not invertible.** `POST /api/blocked_by {item_id,
# blocker_id}` reads exactly as "*item_id* is blocked by *blocker_id*".
# Linear's inverted `issueRelationCreate` recipe, and the standing warning
# that getting its direction backwards silently inverts the frontier, are
# both deleted — there is no direction to get backwards here.
#
# ---------------------------------------------------------------------------
# Three shapes fall out of the API and are worth stating once.
#
# 1. **Every read is the whole sweep.** There is no `GET /api/items/:id` or
#    `GET /api/projects` — `GET /api/sweep` is the only read of domain data,
#    so each read verb fetches once and filters in jq.
#
# 2. **`HB-<seq>` is a client-side affordance.** No route accepts or
#    resolves it; `seq` is server-minted and appears only in `Item.seq`.
#    `resolve_ref` maps it onto a uuid off the sweep already fetched.
#
# 3. **A write is CAS, retried exactly once.** Read `version`, `PATCH` with
#    `expected_version`, and on a 409 compare touched fields with the
#    original row. Disjoint changes are resent against the carried current
#    version; same-field changes stop rather than overwrite.
#
# **Batch mints are replay-safe by construction.** Creates are idempotent by
# client-supplied id, and `mint` derives every id it does not find in the
# manifest deterministically from the project and the title — so a re-run
# after a partial failure replays identically instead of double-minting.
# That is what lets the whole batch be one confirmed pass with no
# bookkeeping between halves of it.
#
# This script is self-contained, and the duplication with the other skills'
# `hb.sh` is deliberate: `runner/Dockerfile` bakes skill directories in
# individually, so a shared helper outside this tree would not ship.
set -euo pipefail

command -v jq >/dev/null || { echo "hb.sh: jq is required" >&2; exit 1; }
command -v curl >/dev/null || { echo "hb.sh: curl is required" >&2; exit 1; }

API_BASE="${HB_API_BASE:-https://hb.twinion.net}"
TOKEN_PATH="${HB_API_TOKEN_PATH:-$HOME/.config/hummingbird/api-token}"

# Frozen, and disjoint from every other id space by the same hash-domain
# separation `client/core/src/sync/write/id.rs` and `sweep.py` rely on.
# Changing it re-mints every project, action and fog row this skill has
# ever written.
ID_NAMESPACE="hummingbird-skill/to-actions/v1"

die() { echo "hb.sh: $*" >&2; exit 1; }

token() {
  # One provisioning line, non-zero exit, no stack trace and no offer to
  # mint one. Read from a file and passed in a header — never on a command
  # line, where `ps` and shell history would both see it.
  [[ -f "$TOKEN_PATH" ]] || die "no authority token at $TOKEN_PATH — mint a device-scope token against $API_BASE (POST /api/admin/tokens, ADMIN_SECRET) and save it to that path"
  tr -d '[:space:]' <"$TOKEN_PATH"
}

# One request. Leaves the body in $BODY and the status in $STATUS, and
# asserts neither — a 409 is an outcome here, not a failure.
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
  BODY=${raw%$'\n'*}
  STATUS=${raw##*$'\n'}
}

sha256_hex() {
  if command -v sha256sum >/dev/null; then sha256sum | cut -d' ' -f1
  else shasum -a 256 | cut -d' ' -f1; fi
}

# `sha256(namespace + seed)`, first 16 bytes, version/variant nibbles forced
# into UUID v4 shape — `deterministic_id`'s recipe, in bash.
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

resolve_ref() { # HB-42 | uuid -> item uuid
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

resolve_project() { # uuid | exact name -> project uuid
  local ref=$1
  sweep | jq -er --arg ref "$ref" \
    'first(.projects[] | select(.id == $ref or .name == $ref) | .id) // empty' \
    || die "no project matching '$ref' — create it with project-create first"
}

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

now_ms() { printf '%s' "$(( $(date +%s) * 1000 ))"; }

# The whole Route, assembled from the four tables that replaced its four
# markdown sections.
project_view() { # project uuid
  sweep | jq --arg p "$1" '
    . as $s
    | [$s.items[] | select(.project_id == $p and .archived_at == null)] as $acts
    | ($acts | map(.id)) as $act_ids
    | {
        project: first($s.projects[] | select(.id == $p)),
        route:   first($s.routes[]   | select(.project_id == $p)),
        fog:     ([$s.fog[] | select(.project_id == $p and .resolved_at == null)]
                  | sort_by(.position, .id)),
        fog_resolved: ([$s.fog[] | select(.project_id == $p and .resolved_at != null)]
                       | sort_by(.position, .id)),
        actions: ($acts | sort_by(.project_pos // 0, .created_at, .id)),
        # Only the edges into actions of this project. A blocker from
        # outside it is still reported — that is a real dependency the
        # Route has to respect — but unrelated edges are not.
        blocked_by: [$s.blocked_by[]
                     | select(.removed_at == null)
                     | select(.item_id as $i | $act_ids | index($i) != null)]
      }
  '
}

cmd="${1:-}"
shift || true

case "$cmd" in
  project-find)
    name="${1:?usage: hb.sh project-find <name>}"
    # A substring, case-insensitive match, because the caller is typing a
    # human name rather than pasting an id. Every match comes back — the
    # skill picks, this does not guess.
    matches=$(sweep | jq --arg n "$name" \
      '[.projects[] | select(.archived_at == null)
        | select(.name | ascii_downcase | contains($n | ascii_downcase))]')
    count=$(jq 'length' <<<"$matches")
    if [[ "$count" == "0" ]]; then
      jq -n --arg n "$name" '{matches: [], searched: $n}'
      exit 0
    fi
    jq -n --argjson views "$(
      jq -r '.[].id' <<<"$matches" | while IFS= read -r pid; do project_view "$pid"; done | jq -s '.'
    )" '{matches: $views}'
    ;;

  project-create)
    name="${1:?usage: hb.sh project-create <name>}"
    id=$(deterministic_id "project/$name")
    request POST /api/projects "$(jq -n --arg id "$id" --arg n "$name" '{id: $id, name: $n}')"
    case "$STATUS" in
      # Idempotent by client id; a replay is the stored row, not a duplicate.
      201|200) printf '%s\n' "$BODY" ;;
      *) die "POST /api/projects answered $STATUS: $BODY" ;;
    esac
    ;;

  route-set)
    ref="${1:?usage: hb.sh route-set <project-ref> [--destination <file>] [--notes <file>]}"
    shift
    dest_file="" notes_file="" clear_dest=false clear_notes=false
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --destination)       dest_file="$2"; shift 2 ;;
        --notes)             notes_file="$2"; shift 2 ;;
        --clear-destination) clear_dest=true; shift ;;
        --clear-notes)       clear_notes=true; shift ;;
        *) die "unknown option $1" ;;
      esac
    done
    pid=$(resolve_project "$ref")

    # Absent leaves the field untouched, `--clear-*` nulls it, a file sets
    # it — the same absent/null/value distinction `RoutePatch`'s own
    # double-`Option` carries, spelled out here so a caller cannot
    # accidentally blank a field it merely did not mention.
    fields='{}'
    if [[ -n "$dest_file" ]]; then
      [[ -f "$dest_file" ]] || die "no destination file at $dest_file"
      fields=$(jq --rawfile d "$dest_file" '. + {destination: ($d | sub("\\s+$"; ""))}' <<<"$fields")
    elif [[ "$clear_dest" == true ]]; then
      fields=$(jq '. + {destination: null}' <<<"$fields")
    fi
    if [[ -n "$notes_file" ]]; then
      [[ -f "$notes_file" ]] || die "no notes file at $notes_file"
      fields=$(jq --rawfile n "$notes_file" '. + {notes: ($n | sub("\\s+$"; ""))}' <<<"$fields")
    elif [[ "$clear_notes" == true ]]; then
      fields=$(jq '. + {notes: null}' <<<"$fields")
    fi
    [[ "$fields" != "{}" ]] || die "route-set was given nothing to set"

    route=$(sweep | jq -er --arg p "$pid" \
      'first(.routes[] | select(.project_id == $p)) // empty') \
      || die "project $pid has no route row — creating a project creates one, so this is a bug, not a missing step"
    cas_patch "/api/routes/$pid" "$fields" "$route"
    ;;

  fog-add)
    ref="${1:?usage: hb.sh fog-add <project-ref> <question>}"
    question="${2:?usage: hb.sh fog-add <project-ref> <question>}"
    pid=$(resolve_project "$ref")
    pos=$(sweep | jq --arg p "$pid" \
      '([.fog[] | select(.project_id == $p) | .position] | max // 0) + 1')
    id=$(deterministic_id "fog/$pid/$question")
    request POST /api/fog "$(jq -n --arg id "$id" --arg p "$pid" \
      --arg q "$question" --argjson pos "$pos" \
      '{id: $id, project_id: $p, question: $q, position: $pos}')"
    case "$STATUS" in
      201|200) printf '%s\n' "$BODY" ;;
      *) die "POST /api/fog answered $STATUS: $BODY" ;;
    esac
    ;;

  fog-resolve)
    id="${1:?usage: hb.sh fog-resolve <fog-id>}"
    row=$(sweep | jq -er --arg id "$id" 'first(.fog[] | select(.id == $id)) // empty') \
      || die "no fog row $id in the sweep"
    if [[ "$(jq -r '.resolved_at' <<<"$row")" != "null" ]]; then
      printf '%s\n' "$row"
      exit 0
    fi
    cas_patch "/api/fog/$id" "$(jq -n --argjson t "$(now_ms)" '{resolved_at: $t}')" "$row"
    ;;

  mint)
    manifest="${1:?usage: hb.sh mint <manifest-file>}"
    [[ -f "$manifest" ]] || die "no manifest at $manifest"
    jq -e 'type == "array"' "$manifest" >/dev/null 2>&1 \
      || die "$manifest must be a JSON array of CreateItem objects"

    # Every id is settled BEFORE the first write, which is what makes a
    # partial batch replayable: the same manifest run twice mints the same
    # ids and the second run is all 200s. An `id` supplied in the manifest
    # wins, so a caller that wants to pin one can.
    prepared=$(jq -c '.[]' "$manifest" | while IFS= read -r entry; do
      normalized=$(jq -c '
        if type != "object" then
          error("every manifest entry must be an object")
        elif (.title? | type) != "string" then
          error("every manifest entry needs a title")
        elif (.title | length) == 0 then
          error("every manifest entry needs a non-empty title")
        elif has("agent") then
          error("agent is owned by /next-up-hb and is forbidden here")
        elif has("stage") and .stage != "ready" then
          error("stage must be ready")
        else
          . + {stage: "ready"}
        end
      ' <<<"$entry") || die "$manifest contains an invalid entry: $entry"
      title=$(jq -r '.title' <<<"$normalized")
      project=$(jq -r '.project_id // ""' <<<"$normalized")
      existing=$(jq -r '.id // ""' <<<"$normalized")

      # ADR-0030 decision 3, copy-at-mint: an action naming no context of
      # its own is filled with its project's default_context, becoming the
      # item's own value rather than a standing read-time join. An action
      # that already names a context (even one that later turns out to
      # match the default) is left alone. This only ever fills a field —
      # it never changes `project`/`title`, so the id derived below is
      # unaffected and a re-run still addresses the same row.
      if [[ -n "$project" ]] && ! jq -e 'has("context") and .context != null' <<<"$normalized" >/dev/null; then
        default_ctx=$(sweep | jq -r --arg p "$project" \
          'first(.projects[] | select(.id == $p) | .default_context) // empty')
        [[ -z "$default_ctx" ]] || normalized=$(jq -c --arg c "$default_ctx" '. + {context: $c}' <<<"$normalized")
      fi

      if [[ -z "$existing" ]]; then
        normalized=$(jq -c --arg id "$(deterministic_id "item/$project/$title")" '. + {id: $id}' <<<"$normalized")
      fi
      printf '%s\n' "$normalized"
    done)

    printf '%s\n' "$prepared" | while IFS= read -r entry; do
      [[ -n "$entry" ]] || continue
      request POST /api/items "$entry"
      case "$STATUS" in
        201|200) printf '%s\n' "$BODY" ;;
        *) die "POST /api/items answered $STATUS for $(jq -r '.title' <<<"$entry"): $BODY" ;;
      esac
    done | jq -s '.'
    ;;

  archive)
    ref="${1:?usage: hb.sh archive <ref>}"
    uuid=$(resolve_ref "$ref")
    row=$(sweep | jq -er --arg id "$uuid" 'first(.items[] | select(.id == $id)) // empty') \
      || die "no item $uuid in the sweep"
    if [[ "$(jq -r '.archived_at' <<<"$row")" != "null" ]]; then
      printf '%s\n' "$row"
      exit 0
    fi
    cas_patch "/api/items/$uuid" "$(jq -n --argjson t "$(now_ms)" '{archived_at: $t}')" "$row"
    ;;

  block)
    ref="${1:?usage: hb.sh block <ref> <blocker-ref>}"
    blocker="${2:?usage: hb.sh block <ref> <blocker-ref>}"
    # Reads exactly as it is written: <ref> is blocked by <blocker-ref>.
    item_id=$(resolve_ref "$ref")
    blocker_id=$(resolve_ref "$blocker")
    [[ "$item_id" != "$blocker_id" ]] || die "an item cannot block itself ($ref)"
    request POST /api/blocked_by "$(jq -n --arg i "$item_id" --arg b "$blocker_id" \
      '{item_id: $i, blocker_id: $b}')"
    case "$STATUS" in
      # The edge's identity is the pair, so the create is idempotent by
      # construction and a re-add clears `removed_at` on the same row.
      201|200) printf '%s\n' "$BODY" ;;
      *) die "POST /api/blocked_by answered $STATUS: $BODY" ;;
    esac
    ;;

  *)
    cat >&2 <<'USAGE'
usage: hb.sh project-find <name>
       hb.sh project-create <name>
       hb.sh route-set <project-ref> [--destination <file>] [--notes <file>]
                                     [--clear-destination] [--clear-notes]
       hb.sh fog-add <project-ref> <question>
       hb.sh fog-resolve <fog-id>
       hb.sh mint <manifest-file>
       hb.sh block <ref> <blocker-ref>
       hb.sh archive <ref>
USAGE
    exit 2
    ;;
esac
