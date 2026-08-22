#!/usr/bin/env bash
# SCPS mail writer for the OpenClaw `scps` skill (ADR-0032).
#
# Usage: scps.sh list YYYY-MM
#        scps.sh add --kind meeting|activity|happy-hour --start <when>
#                     [--topic "…"] [--end <when> | --duration MIN]
#                     [--where "…"] [--notes "…"]
#        scps.sh update <event-id> [--kind …] [--topic …] [--start …]
#                        [--end …] [--where …] [--notes …]
#        scps.sh quest YYYY-MM "<phrase>"
#
# `<when>` is `YYYY-MM-DDTHH:MM`, exactly `gcal.sh`'s shape.
#
# # Two writers, two credentials, deliberately not shared
#
# `list`/`add`/`update` are Google Calendar work and are **delegated to
# `gcal.sh`** (`openclaw/calendar/scripts/gcal.sh`), resolved as a sibling
# skill directory the same way `{baseDir}` resolves this skill's own
# scripts — never a copy. This script contains no Google token mint and no
# copy of `gcal.sh`'s frozen event-id recipe; the openclaw workflow's parity
# check over that recipe is untouched by anything here.
#
# `quest` is authority work (`PUT /api/settings/scps-quest`) and, like every
# other OpenClaw skill script, carries its own small `token`/`request` pair
# against `~/.config/hummingbird/api-token` — the deliberate per-script
# duplication `docs/openclaw.md`'s shape table already names, not a slip.
#
# # Titles are the script's alone
#
# `scps_title` is the only place a title is built, from `--kind` and
# `--topic`. There is no verb that accepts a raw `--title` — the model
# extracts kind, topic, time and place from the mail; the script decides
# the words that land on the calendar.
set -euo pipefail

command -v jq >/dev/null || { echo "scps.sh: jq is required" >&2; exit 1; }
command -v curl >/dev/null || { echo "scps.sh: curl is required" >&2; exit 1; }

API_BASE="${HB_API_BASE:-https://hb.twinion.net}"
TOKEN_PATH="${HB_API_TOKEN_PATH:-$HOME/.config/hummingbird/api-token}"

# This skill's own directory, then its sibling `calendar` skill's script.
# Installed skills sit as siblings under one skills root
# (`<root>/<skill>/scripts/<script>.sh`), the same layout every `{baseDir}`
# in these SKILL.md files assumes — so climbing two levels off this script
# and back down into `calendar/scripts` is the same resolution the charter
# does for `{baseDir}` itself, just done in bash instead of by the runtime.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SKILLS_ROOT="$(dirname "$(dirname "$SCRIPT_DIR")")"
GCAL="${HB_GCAL_SH:-$SKILLS_ROOT/calendar/scripts/gcal.sh}"

die() { echo "scps.sh: $*" >&2; exit 1; }

[[ -x "$GCAL" ]] || die "gcal.sh not found (or not executable) at $GCAL — expected as the sibling skill openclaw/calendar/scripts/gcal.sh; override with HB_GCAL_SH"

token() {
  [[ -f "$TOKEN_PATH" ]] || die "no authority token at $TOKEN_PATH — mint a device-scope token against $API_BASE (POST /api/admin/tokens, ADMIN_SECRET) and save it to that path"
  tr -d '[:space:]' <"$TOKEN_PATH"
}

# One request to the authority (the `quest` verb only). Leaves the body in
# $BODY and the status in $STATUS, and does not assert either.
request() { # method path [json-body]
  local method=$1 path=$2 data=${3:-} raw auth_token curl_status
  auth_token=$(token)
  [[ -n "$auth_token" ]] || die "empty authority token at $TOKEN_PATH"
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

valid_datetime() { [[ "$1" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}$ ]]; }
valid_month() { [[ "$1" =~ ^[0-9]{4}-(0[1-9]|1[0-2])$ ]]; }

# `date -d`/`date -j` both in scope — the gateway is WSL (GNU), a session
# on the operator's Mac reads the same file (gcal.sh's own note).
month_end() { # YYYY-MM-01 -> last day of that month, YYYY-MM-DD
  if date -d "$1 +1 month -1 day" +%Y-%m-%d 2>/dev/null; then :
  else date -j -v+1m -v-1d -f '%Y-%m-%d' "$1" +%Y-%m-%d; fi
}

days_between() { # YYYY-MM-DD YYYY-MM-DD -> whole days, b - a
  local ea eb
  if ea=$(date -d "$1" +%s 2>/dev/null); then
    eb=$(date -d "$2" +%s)
  else
    ea=$(date -j -f '%Y-%m-%d' "$1" +%s)
    eb=$(date -j -f '%Y-%m-%d' "$2" +%s)
  fi
  echo $(( (eb - ea) / 86400 ))
}

# `SCPS Meeting: <topic>` / `SCPS Activity: <topic>` /
# `SCPS Happy Hour[: <topic>]` — the pane↔writer title contract (ADR-0032
# decision 2). The only function that builds a title; every verb below
# routes through it rather than accepting one as an argument.
scps_title() { # kind topic(optional) -> title
  local kind=$1 topic=${2:-}
  case "$kind" in
    meeting)
      [[ -n "$topic" ]] || die "--topic is required for --kind meeting"
      printf 'SCPS Meeting: %s' "$topic" ;;
    activity)
      [[ -n "$topic" ]] || die "--topic is required for --kind activity"
      printf 'SCPS Activity: %s' "$topic" ;;
    happy-hour)
      if [[ -n "$topic" ]]; then printf 'SCPS Happy Hour: %s' "$topic"
      else printf 'SCPS Happy Hour'; fi ;;
    "") die "--kind is required (meeting, activity or happy-hour)" ;;
    *) die "--kind must be one of: meeting activity happy-hour (got $kind)" ;;
  esac
}

# The reverse of scps_title, for rendering `list`/`add`/`update` output —
# best-effort only, over whatever title actually comes back from Google
# (which could predate this skill, or be hand-edited).
kind_of_title() { # title -> meeting|activity|happy-hour|other
  case "$1" in
    "SCPS Meeting: "*)    echo meeting ;;
    "SCPS Activity: "*)   echo activity ;;
    "SCPS Happy Hour"*)   echo happy-hour ;;
    *)                    echo other ;;
  esac
}

default_duration() { # kind -> minutes
  case "$1" in
    meeting) echo 120 ;;
    activity) echo 180 ;;
    happy-hour) echo 60 ;;
  esac
}

# `gcal.sh add`'s success line is `start\ttitle\tid`; its 409 dedupe line is
# `already on the calendar\tstart\ttitle\tid`. `gcal.sh edit`'s line is
# always the 3-field success shape. Reformat either into this skill's own
# `id\tkind\tstart\ttitle` order (the brief's own column order for `list`).
render_gcal_line() { # kind raw-gcal-output
  local kind=$1 raw=$2 f1 f2 f3 f4
  IFS=$'\t' read -r f1 f2 f3 f4 <<<"$raw"
  if [[ "$f1" == "already on the calendar" ]]; then
    printf 'already on the calendar\t%s\t%s\t%s\t%s\n' "$f4" "$kind" "$f2" "$f3"
  else
    printf '%s\t%s\t%s\t%s\n' "$f3" "$kind" "$f1" "$f2"
  fi
}

cmd="${1:-}"
shift || true

case "$cmd" in
  list)
    month="${1:?usage: scps.sh list YYYY-MM}"
    shift || true
    valid_month "$month" || die "month must be YYYY-MM"
    first_day="${month}-01"
    end_day=$(month_end "$first_day")
    today=$(date -u +%Y-%m-%d)
    days=$(days_between "$today" "$end_day")
    [[ "$days" -ge 0 ]] || die "$month is entirely in the past — list only reaches from today forward, the same window gcal.sh agenda has"
    # +1: gcal.sh agenda's window is [now, now+days) as an instant, so the
    # last calendar day of the month needs one more day of headroom to be
    # fully inside it. Note this still cannot see a day *earlier this
    # month* than today — agenda's own window starts at "now", never at
    # the start of a month; a mid-month list therefore cannot recover an
    # event dated earlier this same month, only this month onward.
    raw=$("$GCAL" agenda --days "$((days + 1))")
    printf '%s\n' "$raw" | awk -F'\t' -v first="$first_day" -v last="$end_day" '
      { start_date = substr($1, 1, 10)
        if (start_date != "" && start_date >= first && start_date <= last && index($2, "SCPS ") == 1)
          print $3 "\t" $2 "\t" $1 }
    ' | while IFS=$'\t' read -r id title start; do
      printf '%s\t%s\t%s\t%s\n' "$id" "$(kind_of_title "$title")" "$start" "$title"
    done
    ;;

  add)
    kind="" start="" topic="" end="" duration="" where="" notes=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --kind)     [[ -n "${2:-}" ]] || die "--kind needs a value"; kind=$2; shift 2 ;;
        --start)    [[ -n "${2:-}" ]] || die "--start needs a value"; start=$2; shift 2 ;;
        --topic)    [[ -n "${2:-}" ]] || die "--topic needs a value"; topic=$2; shift 2 ;;
        --end)      [[ -n "${2:-}" ]] || die "--end needs a value"; end=$2; shift 2 ;;
        --duration) [[ "${2:-}" =~ ^[0-9]+$ ]] || die "--duration is in whole minutes"; duration=$2; shift 2 ;;
        --where)    [[ -n "${2:-}" ]] || die "--where needs a value"; where=$2; shift 2 ;;
        --notes)    [[ -n "${2:-}" ]] || die "--notes needs a value"; notes=$2; shift 2 ;;
        *) die "unknown flag $1" ;;
      esac
    done
    [[ -n "$kind" ]] || die "add needs --kind meeting|activity|happy-hour"
    [[ -n "$start" ]] || die "add needs --start"
    valid_datetime "$start" || die "--start takes YYYY-MM-DDTHH:MM"
    [[ -z "$end" || -z "$duration" ]] || die "--end and --duration are alternatives, not both"
    [[ -z "$end" ]] || valid_datetime "$end" || die "--end takes YYYY-MM-DDTHH:MM"

    title=$(scps_title "$kind" "$topic")
    [[ -n "$end" || -n "$duration" ]] || duration=$(default_duration "$kind")

    gcal_args=("$title" --start "$start")
    [[ -n "$end" ]] && gcal_args+=(--end "$end")
    [[ -n "$duration" ]] && gcal_args+=(--duration "$duration")
    [[ -n "$where" ]] && gcal_args+=(--where "$where")
    [[ -n "$notes" ]] && gcal_args+=(--notes "$notes")

    out=$("$GCAL" add "${gcal_args[@]}")
    render_gcal_line "$kind" "$out"
    ;;

  update)
    id="${1:?usage: scps.sh update <event-id> [--kind …] [--topic …] [--start …] [--end …] [--where …] [--notes …]}"
    shift || true
    kind="" topic="" start="" end="" where="" notes=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --kind)   [[ -n "${2:-}" ]] || die "--kind needs a value"; kind=$2; shift 2 ;;
        --topic)  [[ -n "${2:-}" ]] || die "--topic needs a value"; topic=$2; shift 2 ;;
        --start)  [[ -n "${2:-}" ]] || die "--start needs a value"; start=$2; shift 2 ;;
        --end)    [[ -n "${2:-}" ]] || die "--end needs a value"; end=$2; shift 2 ;;
        --where)  [[ -n "${2:-}" ]] || die "--where needs a value"; where=$2; shift 2 ;;
        --notes)  [[ -n "${2:-}" ]] || die "--notes needs a value"; notes=$2; shift 2 ;;
        --duration) die "update has no --duration — pass --end to change the length, or --start alone to keep it (gcal.sh edit preserves the event's own duration)" ;;
        *) die "unknown flag $1" ;;
      esac
    done

    gcal_args=("$id")
    if [[ -n "$kind" || -n "$topic" ]]; then
      [[ -n "$kind" ]] || die "update needs --kind alongside --topic to build a title"
      gcal_args+=(--title "$(scps_title "$kind" "$topic")")
    fi
    if [[ -n "$start" ]]; then
      valid_datetime "$start" || die "--start takes YYYY-MM-DDTHH:MM"
      gcal_args+=(--start "$start")
    fi
    if [[ -n "$end" ]]; then
      valid_datetime "$end" || die "--end takes YYYY-MM-DDTHH:MM"
      gcal_args+=(--end "$end")
    fi
    [[ -n "$where" ]] && gcal_args+=(--where "$where")
    [[ -n "$notes" ]] && gcal_args+=(--notes "$notes")
    [[ ${#gcal_args[@]} -gt 1 ]] || die "update needs at least one field to change"

    out=$("$GCAL" edit "${gcal_args[@]}")
    IFS=$'\t' read -r r_start r_title r_id <<<"$out"
    printf '%s\t%s\t%s\t%s\n' "$r_id" "$(kind_of_title "$r_title")" "$r_start" "$r_title"
    ;;

  quest)
    month="${1:?usage: scps.sh quest YYYY-MM \"<phrase>\"}"
    shift || true
    phrase="${1:?usage: scps.sh quest YYYY-MM \"<phrase>\"}"
    shift || true
    valid_month "$month" || die "month must be YYYY-MM"
    [[ -n "$phrase" ]] || die "phrase must not be empty"
    [[ "$phrase" != *$'\n'* ]] || die "phrase must not contain a newline"

    value="$month $phrase"
    request GET /api/settings/scps-quest
    case "$STATUS" in
      200) version=$(jq -er '.version' <<<"$BODY") || die "GET /api/settings/scps-quest answered 200 with no version: $BODY" ;;
      404) version=0 ;;
      *) die "GET /api/settings/scps-quest answered $STATUS: $BODY" ;;
    esac

    payload=$(jq -nc --arg v "$value" --argjson ev "$version" '{value: $v, expected_version: $ev}')
    request PUT /api/settings/scps-quest "$payload"
    case "$STATUS" in
      200|201) jq -r '"\(.value)\t(version \(.version))"' <<<"$BODY" ;;
      409)
        # One bounded retry, `hb-tasks.sh`'s `cas_patch` shape: another
        # writer moved the row between the read and the write. If it now
        # already holds the value we wanted, that is success, not a
        # conflict — the CAS-correctness rule this verb exists to satisfy.
        current_value=$(jq -er '.current.value' <<<"$BODY") || die "PUT /api/settings/scps-quest 409 carried no current entity: $BODY"
        if [[ "$current_value" == "\"$value\"" || "$current_value" == "$value" ]]; then
          jq -r '"\(.current.value)\t(version \(.current.version))"' <<<"$BODY"
          exit 0
        fi
        retry_version=$(jq -er '.current.version' <<<"$BODY") || die "PUT /api/settings/scps-quest 409 carried no current version: $BODY"
        payload=$(jq -nc --arg v "$value" --argjson ev "$retry_version" '{value: $v, expected_version: $ev}')
        request PUT /api/settings/scps-quest "$payload"
        case "$STATUS" in
          200|201) jq -r '"\(.value)\t(version \(.version))"' <<<"$BODY" ;;
          409) die "PUT /api/settings/scps-quest still conflicting after one retry — another writer is moving scps-quest; re-read and decide: $BODY" ;;
          *) die "PUT /api/settings/scps-quest answered $STATUS: $BODY" ;;
        esac
        ;;
      *) die "PUT /api/settings/scps-quest answered $STATUS: $BODY" ;;
    esac
    ;;

  *)
    cat >&2 <<'USAGE'
usage: scps.sh list YYYY-MM
       scps.sh add --kind meeting|activity|happy-hour --start YYYY-MM-DDTHH:MM
                    [--topic "…"] [--end YYYY-MM-DDTHH:MM | --duration MIN]
                    [--where "…"] [--notes "…"]
       scps.sh update <event-id> [--kind …] [--topic …] [--start …]
                       [--end …] [--where …] [--notes …]
       scps.sh quest YYYY-MM "<phrase>"

Event ids come from `list` and nowhere else. `--kind`/`--topic` build the
title; there is no way to pass a raw title.
USAGE
    exit 2
    ;;
esac
