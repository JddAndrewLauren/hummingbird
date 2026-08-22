---
name: scps
description: Extract Sierra Club Photo Section mail into John's Google Calendar and the monthly Photo Quest binding. Use when a mail from the section's sender arrives in the agent's SCPS mailbox.
---

# SCPS mail (OpenClaw arm, ADR-0032)

The Sierra Club Photo Section's mail, through `scripts/scps.sh`. This skill
writes two things: `SCPS `-prefixed events on John's primary Google
Calendar (through `calendar`'s own `gcal.sh` — see below) and the monthly
Photo Quest, a `settings` binding read by the standing-question pane. It
never reads mail itself; the agent's own mail tools do that, and hand this
skill only what it decided to extract.

## The verbs

```bash
{baseDir}/scripts/scps.sh list 2026-08
{baseDir}/scripts/scps.sh add --kind meeting --start 2026-08-15T14:00 \
    --topic "Impressions of Venice, Italy" --notes "1:45 p.m. get together"
{baseDir}/scripts/scps.sh add --kind activity --start 2026-08-22T09:00 \
    --topic "Super Girl Surf Festival" --where "Oceanside"
{baseDir}/scripts/scps.sh add --kind happy-hour --start 2026-03-05T19:00
{baseDir}/scripts/scps.sh update <event-id> --start 2026-08-22T10:00
{baseDir}/scripts/scps.sh quest 2026-08 "Depth of Field (brought to you by Kevin)"
```

- **`list YYYY-MM`** is the only read, and the only source of event ids —
  never guess one, the same rule `calendar` follows. It shows every event
  that month whose title starts with `SCPS ` (exact prefix, case-sensitive),
  as `id`, `kind`, `start`, `title`. It reuses `gcal.sh agenda`'s window,
  which starts at *now* — so it reaches this month forward, never a month
  already fully past, and cannot recover an event dated earlier this same
  month than today. Run it for every month a mail covers before deciding
  add vs. update.
- **`add`** builds the title from `--kind` + `--topic` — you never pass a
  raw title. `--topic` is required for `meeting` and `activity`; optional
  for `happy-hour` (bare `SCPS Happy Hour` with none). Default durations:
  meeting 2h, activity 3h, happy hour 1h — pass `--end` or `--duration`
  only when the mail states a different length.
- **`update <event-id>`** retitles/moves/edits an event `list` found. Pass
  `--kind`/`--topic` together to change the title; `--start` alone keeps
  the event's own length (`gcal.sh edit`'s own rule). No `--duration` here
  — pass `--end` for an explicit new length.
- **`quest YYYY-MM "<phrase>"`** writes the `scps-quest` setting under CAS.
  Unset, identical, and changed are all handled — call it plainly, it
  reports the stored value and version either way.

## What NOT to write

- `TBD` activities and meetings create nothing.
- Board meetings (`SCPS Board Meeting: …`) are never written — they are not
  section programming.
- A Photo Quest with no `brought to you by` credit still writes; the phrase
  is whatever the mail says.

## Dedupe: month + kind (+ topic), never by day

Activities move, sometimes more than once before they happen. `list` the
month, find an existing event of the same kind — and, when a month has two
of a kind (`Activity #1`/`#2`), the same topic — and `update` it instead of
adding a second one. That is how a moved Activity moves instead of forking.
Otherwise `add`.

## Start time, notes, location

The official start (the one after "start of meeting" / the activity's own
time) is `--start`; a get-together or set-up time named earlier is prose in
`--notes`, not a second event. A named location is `--where`.

## Boundaries

- Calendar writes go through **`gcal.sh`** (`openclaw/calendar/scripts/
  gcal.sh`, resolved as this skill's sibling) — never a second Google
  credential, never a copy of its frozen event-id recipe. This skill's own
  authority token (the same file every OpenClaw skill reads) is used only
  for the `quest` binding write.
- Titles come from `--kind` + `--topic` alone. If a mail's wording does not
  fit `meeting`/`activity`/`happy-hour`, say so rather than forcing a kind.
- Ambiguous dates ("early May", "TBD" that later resolves) — ask John
  rather than guessing a date this skill would then write.
- Report in one line what was written, per month: adds, updates, and the
  quest.
