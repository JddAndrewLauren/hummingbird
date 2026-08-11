---
name: next-up-personal
description: Pick what to do right now from the personal Linear workspace (org twinion, team ION) — survey Ready/In Progress work, rank it by context, energy and time, and present one top pick with a few alternates and a pipeline-health footer; also the front door for handing a single `agent`-labelled issue to an agent. Use when the user asks "what should I do", "what's next", "what can I hand off", invokes /next-up-personal, or names an issue to delegate. Not a decomposer — /to-actions breaks a project into actions, /microtask breaks a picked issue into steps.
---

# /next-up-personal

> **Status:** targets the live Linear workspace, which remains the working
> surface until the owned stack (ADR-0008) is daily-usable; the skill
> retargets to the owned API then.

A **task selector**, not a router: answer "what should I do right now" against the live
Linear workspace with **one** top pick. Plus one branch — handing a single chosen issue to
an agent. Never a full list; that's the Linear UI's job.

Vocabulary (Action, Route, Destination, Fog, External wait) is in the root `CONTEXT.md`.
API mechanics are in [REFERENCE.md](REFERENCE.md).

All Linear reads/writes go through `scripts/linear.sh` (in this skill's directory):

- `linear.sh survey` — one call, one JSON blob: `candidates`, `blocked`, `health`, `projects`
- `linear.sh get <IDENT>` — one issue with description, labels, state, and open `blockers`
- `linear.sh move <IDENT> <state-name>` — state change, resolved by state *name*
- `linear.sh comment <IDENT> <file>` — post a comment from a markdown file
- `linear.sh unlabel <IDENT> agent` — remove a label (idempotent)

The script owns the mechanical filters. **Everything below the survey — ranking, context
narrowing, the fog reading — is judgment, and stays here in prose.**

The survey includes Ready and In Progress work plus non-frontier work that is overdue, due
today, or due within the next seven calendar days. Issues in the `Blocked` state are waiting
on the external world and never enter the actionable candidates.

## Preflight

Missing key file → the script prints one provisioning line and exits non-zero. Relay that
line and stop. No stack trace, no retry, no offer to create the key.

## Invocation

One parser handles every entry point:

- **An issue id** (`ION-12`, case-insensitive `ION-\d+`) → skip the survey entirely and go
  straight to **Delegation**. Required: delegation must work outside a next-up context.
- **Filter args** — any subset of context / energy / time, in any order:
  `office low 30m`, `@computer quick`, `high`. Also `agent` to mean "only what I could
  hand off". Contexts match with or without the `@`. Time is either a duration (`30m`,
  `2h`, `15 min`) or a `size` word (`quick` ≲15 min, `medium`, `deep` needs a real block);
  map a duration onto the nearest size — ≲15 min → `quick`, up to about an hour →
  `medium`, beyond that → `deep`.
- **Bare `/next-up-personal`** → run the chips flow: ask context, energy, and time via the
  in-session multiple-choice prompt, **each axis skippable** ("anywhere" / "either" /
  "any"). One prompt, three axes if the harness allows it; plain text if it doesn't.
  Skipping every axis is fine and common — rank the whole frontier.

Free text ("I've got twenty minutes and no brain") is a fast path: read it into the same
three axes and proceed, don't interrogate.

Any of the above may additionally carry **calendar context** — see below. It's an
optional add-on to the input, not a fifth entry point.

**No standalone GUI.** Deferred to v2 behind the dashboard tripwire — don't smuggle one in.

## Calendar context (optional input)

An **optional** field on top of the three axes above. Its shape is issue #70's
provider-agnostic read contract, verbatim — this skill knows no Google (or any other
provider's) field names, holds no calendar credential, and makes no calendar API call.
Everything below is read-only context; it never becomes a Linear write.

**Schema home:** #41 reserves a per-skill JSON schema versioned beside each `SKILL.md`
once the skill-runner (`runner/`) exists. That home doesn't exist yet in this repo, so
this section is the **interim contract surface** — move it verbatim into the versioned
schema file the first time this skill grows one, don't restate it from memory.

```jsonc
{
  // Mirrors client/core's CurrentOrNext (issue #70): the in-progress event if
  // one is live, else the soonest upcoming one, else neither.
  "current_or_next": {
    "status": "in_progress" | "upcoming" | "none",
    "event": EventRecord | null   // null iff status == "none"
  },
  // The result of client/core's events_overlapping_interval (issue #70) run
  // against today's local calendar day, local-time order. May be empty.
  "today": [EventRecord, ...]
}
```

`EventRecord` is #70's struct, field-for-field, serialized as-is (see
`client/core/src/calendar/event.rs`): `provider_event_id`, `calendar_id`, `title`,
`when`, `recurrence_id`, `location`, `organizer`, `status`,
`provider_updated_at_ms`, `html_link`. No field here is Google-specific and none is
renamed for this skill.

`when` is a two-armed, `kind`-tagged union (ADR-0015's 2026-08-10 amendment), and
which arm an event is on is the *only* place all-day-ness is recorded — there is no
`all_day` flag beside it, and no time zone on either arm:

```jsonc
{"kind": "timed",   "start_ms": 1786551000000, "end_ms": 1786554600000}
{"kind": "all_day", "start_date": "2026-09-09", "end_date": "2026-09-16"}  // end exclusive
```

An all-day event carries the provider's civil dates verbatim and no instant, so it can
never fire the 30-minute nudge — there is no moment to be thirty minutes before.
Flattening one to a midnight instant is the "India in **394** days" defect that
amendment exists to prevent; never do it when composing this block by hand.

**How it's supplied.** The hosted skill-runner (#41) stays context-blind — calendar
context arrives, if at all, in the `{skill, args}` payload from the calling device's
mirror (ADR-0005), which already holds the polled snapshot. Until #73 lands calendar
polling on a real device, a session may supply this field by hand (e.g. pasted JSON) to
exercise the behavior below.

**Field absent → behavior identical to today.** No display line, no ranking change. This
is the default and the common case until #73 ships.

**Field present:**

1. **Display first.** Before the top pick, print one line for `current_or_next`:
   - a timed `in_progress` event → `Now: <title> (until <end local time>)`
   - an all-day `in_progress` event → `Now: <title> (all day)`
   - a timed `upcoming` event → `Next: <title> at <start local time>`
   - an all-day `upcoming` event → `Next: <title> (all day, <start_date>)`
   - `none` → omit the line entirely (nothing to show, not an error)
2. **Soft size-ranking shift, never a filter.** First find **the next timed start**:
   the soonest timed event `when.start_ms` strictly after the declared "now". Use
   `current_or_next.event.when.start_ms` only when that event is both `upcoming` and
   `kind: "timed"`; otherwise read `today` for its earliest timed entry with a
   `when.start_ms` after "now". Do this when the in-progress event is all-day too: an
   all-day event runs all day and would otherwise mask every meeting behind it, which is
   exactly when a 30-minute warning matters most. All-day dates are deliberately not
   converted to instants for this lookup, so an all-day event itself never triggers this
   30-minute shift. If there is no such timed start, there is no shift.

   If the next start is **within 30 minutes** of the declared "now",
   treat it as an added signal inside ranking step 5 (Energy/size fit, below): a
   candidate labeled `size: quick` moves ahead of an otherwise-equally-ranked
   non-`quick` candidate. It **never drops** a `medium` or `deep` candidate from the
   list and never overrides context (step 1), overdue/due-today (step 2), In Progress
   bias (step 3), or priority (step 4) — those still run first, untouched. A `deep`
   chore can still win if nothing else is live; the shift only breaks ties among what
   the earlier steps already left standing.
   - Worked example: two candidates tie through steps 1–4 (`ION-20`, `size: medium`,
     `energy: low`) and (`ION-21`, `size: quick`, `energy: low`), both otherwise equal.
     With no calendar context, step 5's "fits the declared time" (say, `30m` declared)
     already favors `ION-21` — the calendar signal is moot there. The shift matters
     when the *declared* time is loose or skipped (`any`): without an event inside 30
     minutes, `ION-20` and `ION-21` stay tied into step 6 (oldest first). With an
     `upcoming` event 12 minutes out, `ION-21` (`quick`) is promoted ahead of `ION-20`
     for that reason alone — `ION-20` is still offered as an alternate, not dropped.
   - Masking example: a 10:00–11:00 standup is in progress at 10:50, with an 11:00
     review next. `current_or_next` reports `in_progress` (the standup), so reading only
     that field finds no upcoming start and applies no shift — at the exact moment the
     user has ten free minutes and needs a `quick` pick. Taking the next start off
     `today` (11:00, ten minutes out) is what makes the shift fire. An all-day
     "Conference" is the same case, all day long.
   - Beyond that one lookup, `today` (the full-day list) is read-only context for the
     display line's surrounding conversation (e.g. "you're also free after 2pm"). It
     never filters and never ranks on its own; only the 30-minute next-start check does.

## Selector model

Four axes. Only one of them is a filter.

- **Context** — the only **hard** filter. `@office` work is undoable from home. Issues with
  **no context label are doable anywhere and always survive the filter.**
- **Energy** and **time/size** — **soft**: they rank, they never exclude. A `deep` chore
  can still be the pick at low energy if nothing else is live; it just ranks below fits.
- **Agent** (`agent` label) — the fourth axis, *who does this*. **Unlabeled means the
  human does it** (there is no `for-human` label in this vocabulary). Use it to answer
  "what could I hand off right now" — the 9pm case where the honest answer isn't a smaller
  task but *not one of mine*. Orthogonal to the Grilling state: an issue can be
  agent-doable and still need grilling first.

**Untagged issues always still surface.** Missing labels are the normal case, not a
disqualification — a workspace where only well-tagged issues get picked trains nobody to
tag and starves everything else.

## Ranking

Apply in this fixed order to `candidates` (the script has already dropped shut issues and
anything with an open blocker):

1. **Context hard-filter** — drop issues whose context labels are all wrong for the
   declared context. Untagged survives.
2. **Overdue / due today** — `overdue` first, then `dueToday`. These jump the queue.
3. **In Progress bias** — strong but **not absolute**: a started issue outranks a fresh
   one, unless it fails the declared context or badly misfits the declared energy. Finish
   before you start.
4. **Linear-native priority** — rank on `priorityLabel`: Urgent > High > Medium > Low >
   No priority. Never sort on the raw `priority` number; it's inverted and `0` means unset
   (see REFERENCE.md).
5. **Energy / size fit** — matching `energy` and a `size` that fits the declared time.
   When calendar context is supplied and **the next start** (as computed above — off
   `current_or_next` when it is `upcoming`, off `today` when an in-progress or all-day
   event masks it) is within 30 minutes, this step also nudges `size: quick` candidates
   ahead of otherwise-tied non-`quick` ones. Read the next start from that rule, never
   from `current_or_next.status` alone: a 10:50 standup with an 11:00 review next is
   exactly the case that must fire and the status field alone misses. Soft only: it
   breaks ties left by steps 1–4, it never removes a candidate.
6. **Oldest first** — `createdAt` ascending, so nothing quietly rots.

## Output

Three parts, nothing more — plus one optional line first when calendar context was
supplied (see above): the current/next event, before the top pick.

1. **One top pick** — identifier, title, and a **one-line why** naming the actual reason it
   won ("overdue since Tuesday", "already In Progress and fits @computer", "the only
   `quick` thing left on your @calls list").
2. **3–5 alternates** — one line each, identifier + title + the label or date that matters.
3. **A one-line health footer** — `Triage N · Grilling N`, so starvation stays visible, plus
   any fog-exhausted project (below). If the candidate list came back short because of
   `blocked`, say so in that line too: "4 more blocked upstream."

Never print the full list.

### The fog-exhaustion flag

A project whose minted actions are **all shut** (`actionsOpen == 0 && actionsTotal > 0`)
while its Route **still lists real fog** gets one footer line:

> ‹project›: route ends in fog, run `/to-actions`.

**The fog check is a reading, not a regex.** The script hands you the `## Fog` section
verbatim; decide whether it names an actual unknown. "Update Acumatica"'s Fog section reads
`* None — the unknowns are carried inside the two investigation actions`, which is a
non-empty string and **must not flag**. A section that only says "none", "n/a", or points
at already-minted actions is not fog.

The selector **never mints**. It points at `/to-actions`.

## Delegation branch

Reached two ways: `/next-up-personal <issue-id>`, or the user accepting the offer on a pick
that carries `agent`. Offer it on any `agent`-labelled pick — one line, no push.

On a directly-named issue, `linear.sh get` it first and check what came back:
- no `agent` label → say so and ask for an explicit go-ahead before running it anyway;
- non-empty `blockers` → say what's blocking it and stop, unless the user overrides;
- already Done/Canceled → stop.

**Completion protocol — fixed by #10, do not re-decide it:**

1. **On start** — `move <IDENT> "In Progress"`. The visible claim, before doing any work.
2. **Do the work** — read the issue body, do the chore, produce something a human can act
   on in ten seconds (the three quotes and a recommendation, not a research diary).
3. **On finish** — in this order:
   - `comment <IDENT> <file>` with the findings;
   - `move <IDENT> Ready`;
   - `unlabel <IDENT> agent`.
4. **Genuine external blocker only** — waiting on a callback, a form that needs a human
   identity, a credential you can't hold: `comment` what's needed, then
   `move <IDENT> Blocked`. Not for "this was harder than expected", and never for a
   dependency on another issue — that's a `blocked by` relation, minted by `/to-actions`.

**Never Done.** An agent chore *advances* a chore, it doesn't complete it: "compare three
insurance quotes" ends with three quotes and a recommendation; choosing and buying is the
human's. Closing would silently drop the remaining human step. The human takes the decision.

**Always remove `agent` on finish.** `agent` means "there is agent work *left* here", not
"an agent touched this once". Leave it and the next survey re-offers the hand-off and the
agent redoes its own research into a second near-identical comment.

If a step fails mid-protocol, report exactly where it stopped — an issue left In Progress
with no comment is worse than one never claimed. `unlabel` is idempotent, so a re-run of a
half-finished finish is safe.

## The /microtask offer

When the user hesitates on the top pick, or the pick is a known-heavy `deep` chore, offer
`/microtask <IDENT>` — **one line, no obligation**, and drop it if they don't bite. Don't
break the issue down here; that's the other skill's job.

## Failure modes

- **Missing API key** — one provisioning line from the script, relayed verbatim. Stop.
- **GraphQL errors** — Linear returns HTTP 200 with an `errors` array; the script surfaces
  the message and exits non-zero. Report it and stop.
- **`truncated: true` in the survey** — a page limit was hit and the frontier is
  incomplete. Say so in the footer rather than presenting a confident pick.
- **Empty `candidates`** — don't invent one. Say the frontier is empty and point at what's
  actually there: blocked items and their blockers, the Triage/Grilling counts, or
  `/to-actions` if a project's route has run out.

## Out of scope

- **Standalone read-GUI** — v2, behind the dashboard tripwire.
- **Batch orchestration** — permanently out (#10). One issue at a time; a fan-out of
  subagents is not scarce enough to wrap in a skill.
- **New states** — the state machine stands unchanged.
- **Any write beyond the delegation protocol's state / comment / label moves.** No minting,
  no description edits, no Route refresh, no re-labelling to "fix" tagging. The selector
  reads; delegation writes exactly four things.
- **Calendar polling, API calls, and credentials** — the skill only ever reads the
  calendar-context field it's handed (#70's shape, via the device mirror per ADR-0005).
  It never calls a calendar provider itself and never holds a calendar credential.
- **The morning-brief surface** — #46 fixes that surface's own contract; this skill's
  calendar-context field is unrelated even though it reuses #70's same read queries.
