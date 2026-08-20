#!/usr/bin/env bash
# Google Calendar helper for the OpenClaw `calendar` skill (ADR-0031).
#
# Usage: gcal.sh agenda [--days N]                 # default 7
#        gcal.sh add <title> --start <when> [--end <when>|--duration <mins>]
#                            [--all-day] [--where <t>] [--notes <t>]
#        gcal.sh edit <event-id> [--title <t>] [--where <t>] [--notes <t>]
#                                [--start <when>] [--end <when>]
#        gcal.sh move <event-id> --start <when> [--end <when>]
#        gcal.sh cancel <event-id>
#
# `<when>` is `YYYY-MM-DDTHH:MM` (local, in the calendar's own timezone), or
# `YYYY-MM-DD` with --all-day.
#
# # Credential
#
# There is **no Google credential on this machine.** The device token at
# ~/.config/hummingbird/api-token (id `openclaw-agent`) buys a short-lived
# Google bearer from the authority — `POST /api/google/calendar_write_token`,
# which answers 403 for every other device token (ADR-0031) — and that bearer
# lives in a shell variable for the rest of the run: never a file, never an
# argument, never printed. It reaches curl on stdin (`-H @-`), which is the
# only way to keep it off both. One run is far shorter than one token's life,
# so there is no rotation logic here at all.
#
# # Scope guard, enforced here and not only in prose
#
# The only URLs this script can build are
# `…/calendars/<id>/events[/<event-id>]` — `gapi` refuses anything else,
# including `…/calendars/<id>` itself. So there is no calendar-list call, no
# ACL call and no freebusy call reachable from here, and a slip in SKILL.md
# cannot become a request the skill was never allowed to make. The credential
# behind the bearer carries `calendar.events` alone, which independently
# forbids creating calendars or changing who they are shared with — and which
# is also why the calendar's timezone is read off `events.list` rather than
# `calendars.get`, a call that scope cannot make.
#
# `cancel` is a patch to `status: "cancelled"`, never `events.delete` — the
# closest thing to this repo's no-delete posture (ADR-0020) that Google
# offers, and the event stays readable in the API afterwards.
#
# Timezone: every `dateTime` is sent with an explicit `timeZone`, read once
# per run from the calendar (or $HB_GCAL_TZ), and every datetime sum is done
# in that zone too. A bare local datetime with no zone is how an event lands
# an hour off; wall-clock arithmetic in the wrong zone is the other way.
set -euo pipefail

command -v jq >/dev/null || { echo "gcal.sh: jq is required" >&2; exit 1; }
command -v curl >/dev/null || { echo "gcal.sh: curl is required" >&2; exit 1; }

API_BASE="${HB_API_BASE:-https://hb.twinion.net}"
TOKEN_PATH="${HB_API_TOKEN_PATH:-$HOME/.config/hummingbird/api-token}"
GOOGLE_API="https://www.googleapis.com/calendar/v3"
# `primary` is the operator's own calendar. No verb takes a calendar id as
# an argument: pointing this script at a throwaway calendar for a rehearsal
# is an environment decision, not something a chat turn can slip into.
CALENDAR_ID="${HB_GCAL_ID:-primary}"

die() { echo "gcal.sh: $*" >&2; exit 1; }

token() {
  # Read from a file and passed in a header — never on a command line,
  # where `ps` and shell history would both see it.
  [[ -f "$TOKEN_PATH" ]] || die "no authority token at $TOKEN_PATH — mint a device-scope token against $API_BASE (POST /api/admin/tokens, ADMIN_SECRET) and save it to that path"
  tr -d '[:space:]' <"$TOKEN_PATH"
}

# One request to the authority. Leaves the body in $BODY and the status in
# $STATUS, and does NOT assert either.
request() { # method path [json-body]
  local method=$1 path=$2 data=${3:-} raw auth_token curl_status
  auth_token=$(token)
  [[ -n "$auth_token" ]] || die "empty authority token at $TOKEN_PATH — mint a device-scope token against $API_BASE (POST /api/admin/tokens, ADMIN_SECRET) and save it to that path"
  # `-H @-` reads the header off stdin, through a pipe: the token reaches curl
  # without a path anyone can open and without an argv `ps` can read. A
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

# The Google bearer, minted once per run and held in this variable only.
#
# Set by a plain call, never `$(…)`: a variable assigned inside a command
# substitution never reaches the parent, so a subshell caller would silently
# mint a second token per call (hb.sh's header records the same trap for its
# sweep cache, which is a file for exactly this reason — a bearer must not
# be).
GTOKEN=""

ensure_gtoken() {
  [[ -z "$GTOKEN" ]] || return 0
  request POST /api/google/calendar_write_token
  case "$STATUS" in
    200) ;;
    403) die "the authority refused this token calendar write (403) — only the openclaw-agent token may mint a write token (ADR-0031); check which device token is at $TOKEN_PATH" ;;
    401) die "the authority rejected the device token at $TOKEN_PATH (401) — it is missing, revoked or malformed" ;;
    503) die "the authority has no calendar write credential configured (503): $BODY" ;;
    *) die "POST /api/google/calendar_write_token answered $STATUS: $BODY" ;;
  esac
  GTOKEN=$(jq -er '.access_token' <<<"$BODY") \
    || die "the calendar write token response carried no access_token"
  [[ -n "$GTOKEN" ]] || die "the calendar write token response carried an empty access_token"
}

# One request to Google, leaving the body in $BODY and the status in
# $STATUS. `suffix` is everything after `/calendars/<id>` and is checked
# against the scope guard in the header: `/events` or `/events/<id>`, either
# optionally with a query string. Nothing else is constructible from here —
# the events collection is the whole reachable surface.
gapi() { # method suffix [json-body]
  local method=$1 suffix=$2 data=${3:-} raw curl_status
  # The event-id segment carries no `.` and no `/`, so no suffix can walk
  # up the path into another collection once curl normalises it.
  [[ "$suffix" =~ ^/events(/[A-Za-z0-9_-]+)?(\?[A-Za-z0-9_.~%@=\&:+-]*)?$ ]] \
    || die "refusing to build a URL outside calendars/<id>/events: $suffix"
  ensure_gtoken
  # Off disk and off argv — see the note in `request`.
  local args=(-sS -w '\n%{http_code}' --connect-timeout 10 --max-time 30
              -X "$method" -H @- "$GOOGLE_API/calendars/$CALENDAR_ID$suffix")
  [[ -n "$data" ]] && args+=(-H 'Content-Type: application/json' -d "$data")
  if raw=$(printf 'Authorization: Bearer %s\n' "$GTOKEN" | curl "${args[@]}"); then
    :
  else
    curl_status=$?
    return "$curl_status"
  fi
  BODY=${raw%$'\n'*}
  STATUS=${raw##*$'\n'}
}

sha256_hex() { # stdin -> 64 hex chars
  if command -v sha256sum >/dev/null; then sha256sum | cut -d' ' -f1
  else shasum -a 256 | cut -d' ' -f1; fi
}

# ---------------------------------------------------------------------------
# FROZEN — the event-id recipe. Do not change the namespace, the seed
# ordering, the separator, the prefix or the length: an id minted under a
# different recipe is a different event, so a change here double-books
# everything already on the calendar instead of colliding with it. This
# block is the durable content of this skill — it ports byte-for-byte if
# calendar write ever moves to another arm (the runner, the Worker, a chat
# surface in the app), the same way `hb.sh`'s step-id recipe does across the
# three arms of docs/openclaw.md's table.
#
#   sha256("hummingbird-openclaw/gcal/v1" + calendarId + "/" + title + "/" +
#          start), first 32 hex characters, prefixed "hb".
#
# Every character it can produce (0-9, a-f, h, b) is a legal Google event id
# character — the API allows base32hex, i.e. a-v and 0-9 — and 34 characters
# sits inside the 5–1024 length rule. A retried insert after a timeout then
# answers 409 "already exists" instead of double-booking, which is the same
# double-mint discipline the step-id recipe exists for.
EVENT_ID_NAMESPACE="hummingbird-openclaw/gcal/v1"

event_id() { # title start -> google event id
  local hex
  hex=$(printf '%s%s/%s/%s' "$EVENT_ID_NAMESPACE" "$CALENDAR_ID" "$1" "$2" | sha256_hex)
  printf 'hb%s' "${hex:0:32}"
}
# END FROZEN
# ---------------------------------------------------------------------------

# GNU `date -d` and BSD `date -v` are both in scope: the gateway is WSL
# (GNU), and a session on the operator's Mac reads the same file.
utc_stamp() { # days-from-now -> RFC3339 Z
  if date -u -d "+$1 days" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null; then :
  else date -u -v"+$1d" +%Y-%m-%dT%H:%M:%SZ; fi
}

# In $TZID, never the gateway's zone: the argument is a wall clock reading on
# the calendar, and adding minutes to a wall clock is a different sum in a
# zone with a different DST calendar. Gateway in New York, calendar in UTC,
# `--start 2026-11-01T01:30 --duration 60`: the gateway's `date` lands back
# on 01:30 inside its repeated hour and books a zero-length event.
#
# So `resolve_timezone` has to have run — every caller pairs the two, and an
# empty $TZID would mean UTC to `date` without saying so.
shift_datetime() { # YYYY-MM-DDTHH:MM minutes -> YYYY-MM-DDTHH:MM
  [[ -n "$TZID" ]] || die "internal: shift_datetime before resolve_timezone"
  if TZ="$TZID" date -d "${1/T/ } $2 minutes" +%Y-%m-%dT%H:%M 2>/dev/null; then :
  else TZ="$TZID" date -j -v"+$2M" -f '%Y-%m-%dT%H:%M' "$1" +%Y-%m-%dT%H:%M; fi
}

next_day() { # YYYY-MM-DD -> YYYY-MM-DD
  if date -d "$1 +1 day" +%Y-%m-%d 2>/dev/null; then :
  else date -j -v+1d -f '%Y-%m-%d' "$1" +%Y-%m-%d; fi
}

# One of Google's RFC3339 stamps (`2026-08-27T15:00:00-07:00`) as epoch
# seconds. BSD `date` wants the offset without its colon and cannot read
# `Z`, so both are normalised first. Empty output means "could not parse" —
# every caller treats that as a reason to stop, never as a zero.
epoch_of() { # RFC3339 -> seconds, or nothing
  local stamp=$1
  if date -d "$stamp" +%s 2>/dev/null; then return 0; fi
  stamp=${stamp/%Z/+0000}
  stamp=$(printf '%s' "$stamp" | sed -E 's/([+-][0-9]{2}):([0-9]{2})$/\1\2/')
  date -j -f '%Y-%m-%dT%H:%M:%S%z' "$stamp" +%s 2>/dev/null || true
}

valid_datetime() { [[ "$1" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}$ ]]; }
valid_date() { [[ "$1" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; }

# The calendar's own timezone, read once per run. $HB_GCAL_TZ short-circuits
# it.
#
# Read off an events.list response, NOT `calendars.get`: the bearer carries
# `calendar.events` alone, and Google authorises `calendars.get` for
# `calendar`/`calendar.readonly`/`calendar.calendars[.readonly]`/
# `calendar.app.created` only — so that call is a 403 on this credential, and
# every timed write depends on this. `events.list` answers with the same
# read-only `timeZone` field and is authorised by `calendar.events`. One
# event is enough; the items are discarded.
TZID=""

resolve_timezone() {
  [[ -z "$TZID" ]] || return 0
  if [[ -n "${HB_GCAL_TZ:-}" ]]; then
    TZID="$HB_GCAL_TZ"
    return 0
  fi
  gapi GET '/events?maxResults=1'
  [[ "$STATUS" == "200" ]] || die "events.list (for the calendar timezone) answered $STATUS: $BODY"
  TZID=$(jq -er '.timeZone' <<<"$BODY") \
    || die "the events.list response carried no timeZone — set HB_GCAL_TZ"
}

# `{dateTime, timeZone}` or `{date}` — never a bare local datetime.
time_object() { # when all-day? -> json
  if [[ "$2" == "all-day" ]]; then
    jq -nc --arg d "$1" '{date: $d}'
  else
    jq -nc --arg dt "$1:00" --arg tz "$TZID" '{dateTime: $dt, timeZone: $tz}'
  fi
}

render_events='.items[] | select(.status != "cancelled")
  | "\(.start.dateTime // .start.date)\t\(.summary // "(no title)")\t\(.id)"'
render_event='"\(.start.dateTime // .start.date)\t\(.summary // "(no title)")\t\(.id)"'

cmd="${1:-}"
shift || true

case "$cmd" in
  agenda)
    days=7
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --days) [[ "${2:-}" =~ ^[0-9]+$ ]] || die "--days must be a whole number"; days=$2; shift 2 ;;
        *) die "unknown flag $1" ;;
      esac
    done
    ensure_gtoken
    gapi GET "/events?timeMin=$(utc_stamp 0)&timeMax=$(utc_stamp "$days")&singleEvents=true&orderBy=startTime&maxResults=250"
    [[ "$STATUS" == "200" ]] || die "events.list answered $STATUS: $BODY"
    jq -r "$render_events" <<<"$BODY"
    ;;

  add)
    title="${1:?usage: gcal.sh add <title> --start <when> [flags — see gcal.sh with no args]}"
    shift
    [[ -n "$title" ]] || die "title must be non-empty"
    start="" end="" duration="" all_day="" where="" notes=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --start)    [[ -n "${2:-}" ]] || die "--start needs a value"; start=$2; shift 2 ;;
        --end)      [[ -n "${2:-}" ]] || die "--end needs a value"; end=$2; shift 2 ;;
        --duration) [[ "${2:-}" =~ ^[0-9]+$ ]] || die "--duration is in whole minutes"; duration=$2; shift 2 ;;
        --all-day)  all_day="all-day"; shift ;;
        --where)    [[ -n "${2:-}" ]] || die "--where needs a value"; where=$2; shift 2 ;;
        --notes)    [[ -n "${2:-}" ]] || die "--notes needs a value"; notes=$2; shift 2 ;;
        *) die "unknown flag $1" ;;
      esac
    done
    [[ -n "$start" ]] || die "add needs --start"
    [[ -z "$end" || -z "$duration" ]] || die "--end and --duration are alternatives, not both"

    if [[ "$all_day" == "all-day" ]]; then
      valid_date "$start" || die "--all-day takes --start YYYY-MM-DD"
      if [[ -n "$end" ]]; then
        valid_date "$end" || die "--all-day takes --end YYYY-MM-DD"
        # Google's all-day end date is exclusive; the operator means the
        # last day of the event, so add one.
        end=$(next_day "$end")
      else
        end=$(next_day "$start")
      fi
    else
      valid_datetime "$start" || die "--start takes YYYY-MM-DDTHH:MM (or YYYY-MM-DD with --all-day)"
      # Before the `end` arithmetic, which is done in the calendar's zone.
      ensure_gtoken
      resolve_timezone
      if [[ -n "$end" ]]; then
        valid_datetime "$end" || die "--end takes YYYY-MM-DDTHH:MM"
      else
        # An hour is the default a bare "put it on Thursday at 3" means.
        end=$(shift_datetime "$start" "${duration:-60}")
      fi
    fi

    payload=$(jq -nc \
      --arg id "$(event_id "$title" "$start")" \
      --arg summary "$title" \
      --argjson start "$(time_object "$start" "$all_day")" \
      --argjson end "$(time_object "$end" "$all_day")" \
      --arg where "$where" --arg notes "$notes" '
      {id: $id, summary: $summary, start: $start, end: $end}
      | if $where != "" then .location = $where else . end
      | if $notes != "" then .description = $notes else . end')
    gapi POST /events "$payload"
    case "$STATUS" in
      200) jq -r "$render_event" <<<"$BODY" ;;
      # The frozen id at work: a retry after a timeout, or a genuine repeat
      # of the same title at the same time, collides instead of
      # double-booking. Not an error — report it as already present.
      409)
        gapi GET "/events/$(event_id "$title" "$start")"
        [[ "$STATUS" == "200" ]] || die "events.insert answered 409 but the existing event could not be read ($STATUS): $BODY"
        printf 'already on the calendar\t'
        jq -r "$render_event" <<<"$BODY"
        ;;
      *) die "events.insert answered $STATUS: $BODY" ;;
    esac
    ;;

  edit|move)
    id="${1:?usage: gcal.sh $cmd <event-id> [flags — see gcal.sh with no args]}"
    shift
    patch='{}'
    start="" end="" old_start="" old_end="" from="" to=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --start) [[ -n "${2:-}" ]] || die "--start needs a value"; start=$2; shift 2 ;;
        --end)   [[ -n "${2:-}" ]] || die "--end needs a value"; end=$2; shift 2 ;;
        --title) [[ "$cmd" == "edit" ]] || die "move takes --start/--end only; use edit for fields"
                 [[ -n "${2:-}" ]] || die "--title needs a value"
                 patch=$(jq -c --arg v "$2" '. + {summary: $v}' <<<"$patch"); shift 2 ;;
        --where) [[ "$cmd" == "edit" ]] || die "move takes --start/--end only; use edit for fields"
                 [[ -n "${2:-}" ]] || die "--where needs a value"
                 patch=$(jq -c --arg v "$2" '. + {location: $v}' <<<"$patch"); shift 2 ;;
        --notes) [[ "$cmd" == "edit" ]] || die "move takes --start/--end only; use edit for fields"
                 [[ -n "${2:-}" ]] || die "--notes needs a value"
                 patch=$(jq -c --arg v "$2" '. + {description: $v}' <<<"$patch"); shift 2 ;;
        *) die "unknown flag $1" ;;
      esac
    done
    [[ "$cmd" != "move" || -n "$start" ]] || die "move needs --start"

    if [[ -n "$start" || -n "$end" ]]; then
      ensure_gtoken
      resolve_timezone
      if [[ -n "$start" ]]; then
        valid_datetime "$start" || die "--start takes YYYY-MM-DDTHH:MM"
        # A new start with no new end **keeps the event's own length**. The
        # end has to move too (Google rejects an end before its start), and
        # both alternatives are wrong: leaving it lands a negative-length
        # 400, and defaulting to an hour silently reshapes a three-hour
        # meeting. So read the event and preserve its duration — and if it
        # cannot be read, say so rather than guessing.
        if [[ -z "$end" ]]; then
          gapi GET "/events/$id"
          [[ "$STATUS" == "200" ]] || die "events.get answered $STATUS: $BODY"
          old_start=$(jq -r '.start.dateTime // empty' <<<"$BODY")
          old_end=$(jq -r '.end.dateTime // empty' <<<"$BODY")
          [[ -n "$old_start" && -n "$old_end" ]] \
            || die "that event is all-day; move it with --all-day on add, or pass an explicit --end"
          from=$(epoch_of "$old_start") to=$(epoch_of "$old_end")
          [[ -n "$from" && -n "$to" ]] \
            || die "could not read the event's current length ($old_start to $old_end) — pass an explicit --end"
          end=$(shift_datetime "$start" "$(( (to - from) / 60 ))")
        fi
        patch=$(jq -c --argjson v "$(time_object "$start" '')" '. + {start: $v}' <<<"$patch")
      fi
      if [[ -n "$end" ]]; then
        valid_datetime "$end" || die "--end takes YYYY-MM-DDTHH:MM"
        patch=$(jq -c --argjson v "$(time_object "$end" '')" '. + {end: $v}' <<<"$patch")
      fi
    fi
    [[ "$patch" != '{}' ]] || die "$cmd needs at least one flag"

    gapi PATCH "/events/$id" "$patch"
    [[ "$STATUS" == "200" ]] || die "events.patch answered $STATUS: $BODY"
    jq -r "$render_event" <<<"$BODY"
    ;;

  cancel)
    id="${1:?usage: gcal.sh cancel <event-id>}"
    # A status patch, never events.delete — see the header.
    gapi PATCH "/events/$id" '{"status":"cancelled"}'
    [[ "$STATUS" == "200" ]] || die "events.patch (cancel) answered $STATUS: $BODY"
    jq -r '"cancelled\t\(.summary // "(no title)")\t\(.id)"' <<<"$BODY"
    ;;

  *)
    cat >&2 <<'USAGE'
usage: gcal.sh agenda [--days N]
       gcal.sh add <title> --start <when> [--end <when>|--duration <mins>]
                           [--all-day] [--where <t>] [--notes <t>]
       gcal.sh edit <event-id> [--title <t>] [--where <t>] [--notes <t>]
                               [--start <when>] [--end <when>]
       gcal.sh move <event-id> --start <when> [--end <when>]
       gcal.sh cancel <event-id>

<when> is YYYY-MM-DDTHH:MM in the calendar's own timezone, or YYYY-MM-DD
with --all-day. Event ids come from `agenda`.
USAGE
    exit 2
    ;;
esac
