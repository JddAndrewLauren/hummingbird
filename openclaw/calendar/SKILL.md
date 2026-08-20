---
name: calendar
description: Read and write John's Google Calendar — show the week, put something on it, move it, retitle it, cancel it. Use when John names a time for something, asks what his week looks like, or wants an event added, moved or cancelled.
---

# Calendar (OpenClaw arm)

John's Google Calendar, through `scripts/gcal.sh`. This is the OpenClaw
agent's only calendar capability and its only non-authority one (ADR-0031):
the same device token every other skill here reads buys a short-lived Google
bearer from the authority, and the script talks to Google directly. **There
is no Google credential on this machine and nothing here to install.**

The calendar is not the task list. An event is a commitment to be somewhere
at a time; an item is work to do. Something can be both — put the event on
the calendar and capture the item through `hummingbird-tasks` — but never
turn one into the other silently.

## The verbs

```bash
{baseDir}/scripts/gcal.sh agenda [--days N]          # default 7
{baseDir}/scripts/gcal.sh add "Dentist" --start 2026-08-27T15:00 \
    [--end 2026-08-27T16:00 | --duration 45] [--where "…"] [--notes "…"]
{baseDir}/scripts/gcal.sh add "Conference" --all-day --start 2026-09-02 [--end 2026-09-04]
{baseDir}/scripts/gcal.sh edit <event-id> [--title …] [--where …] [--notes …] [--start …] [--end …]
{baseDir}/scripts/gcal.sh move <event-id> --start 2026-08-27T16:30 [--end …]
{baseDir}/scripts/gcal.sh cancel <event-id>
```

- `<when>` is `YYYY-MM-DDTHH:MM` in the calendar's own timezone (read from
  the calendar itself each run) — never a UTC offset, never a bare "3pm".
  Resolve "Thursday at 3" against today's date **yourself**, and say the
  resolved date back to John in your confirmation.
- **Event ids come from `agenda`** and from nowhere else. Never guess one,
  and never show one to John — name an event by its title and time, the
  same rule items follow (`HB-<seq>` is not how you name an item either).
- `--duration` is in minutes. With neither `--end` nor `--duration`, a timed
  event is an hour.
- **`move` and `edit --start` keep the event's own length** — a three-hour
  meeting moved to 9am ends at noon. Pass `--end` only when the length is
  changing too.
- `--all-day --end` is the **last day** of the event, inclusive, in John's
  sense of it; the script converts to Google's exclusive end date.

## Adding twice

`add` mints the event id deterministically from the calendar, the title and
the start, so the same add twice is the *same* event. A repeat reports
`already on the calendar` and changes nothing — that is the safe answer to a
timeout, exactly like the microtask skill's step ids. If John genuinely
wants a second event at that time, give it a different title.

## Confirmation

- `agenda` is free — run it whenever you need the week, unprompted.
- `add` needs no second confirmation when John just asked for it. Read the
  resolved date and time back in your reply so a mis-parse is visible.
- **`edit`, `move` and `cancel` always confirm first** on an event you did
  not create in this session. These change something John or someone else
  put there, possibly with other people expecting it; state the event's
  current title and time, say exactly what will change, and wait.

## Boundaries

- Every calendar call goes through this script. Never call the Google API
  directly and never ask John for a Google credential — the script is
  scope-guarded (only `calendars/<id>/events`, plus one read of the
  calendar's timezone) and the guard is the point.
- The bearer this script mints can create, edit and cancel **events** and
  nothing else. It cannot create calendars, change sharing, or read Gmail.
- **There is no delete.** `cancel` marks an event cancelled and leaves it
  readable, matching the authority's own no-delete posture.
- One calendar per run, `primary` unless the environment says otherwise. No
  verb takes a calendar id, so you cannot write to the wrong calendar by
  choosing wrongly in a chat turn.
- Guests, invitations, recurrence and reminders are out of scope. If John
  asks for one, say the skill does not do it rather than approximating —
  `edit`ing a recurring event through this script touches only the single
  instance id `agenda` returned.
