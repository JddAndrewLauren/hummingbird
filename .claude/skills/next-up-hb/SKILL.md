---
name: next-up-hb
description: Pick what to do right now from the app-owned authority (ADR-0008) — survey what is startable in one read-only call, rank it with the deterministic ranker, and present one top pick with a few alternates and a pipeline-health footer. Use when the user asks "what should I do", "what's next", "what can I do in twenty minutes", or invokes /next-up-hb. Not a decomposer and not a delegator — /to-actions breaks a project into actions, /microtask breaks a picked issue into steps.
---

# /next-up-hb

> **Why the `-hb`:** the operator's profile ships a different `/next-up`
> (pick the next piece of work across registered projects). This is the
> hummingbird-specific one, and the suffix is what keeps the two slash
> commands unambiguous. The runner op name and this directory carry it too.

A **task selector**, not a router: answer "what should I do right now" against the
app-owned authority with **one** top pick. Never a full list.

Vocabulary (Action, Route, Destination, Fog, External wait) is in the root `CONTEXT.md`.

**This skill talks to nothing but hummingbird's own API.** No Linear, no issue ids of any
other tracker, no `scripts/linear.sh` — `/next-up-personal` is the Linear-era skill and is
untouched by this one.

## What this skill does and does not decide

The ranking is **not here**. `client/core/src/rank.rs` (#162) is the single authority for
the six ranking steps, the opposite-pole demotion, the calendar nudge and the reason
codes; `client/next-up` is the crate that calls it. **Do not restate the steps in this
file or re-derive them in prose** — a second copy here is exactly the drift this skill was
built to delete. Read the module doc if you need to explain a result.

What *is* here: parsing the axes out of what the user said, choosing the *why* line,
reading the fog, and writing the answer.

## The two arms

**Interactive** (a session, with the operator's credential):

```
.claude/skills/next-up-hb/scripts/next-up.sh survey \
  [--context @computer] [--energy low|medium|high] [--size quick|short|deep] \
  [--calendar <file>]
```

One read-only `GET /api/sweep`, then the ranker. Everything the survey needs — items,
blocked-by edges, projects, fog — arrives in that one payload.

**Hosted runner** (#41/#256's second op): the runner is **context-blind**. The sweep
payload arrives in the `{skill, args}` request from the calling device's mirror, so the
runner holds no authority token and makes no HTTP call.

**On this arm the ranking has already happened.** The runner spawns `next-up-rank` itself
before you are invoked and puts its answer in your prompt under `ranked` — you have no
shell here, and asking for one wastes the run: `claude -p` is non-interactive, so a `Bash`
call cannot be prompted for and is simply denied. Do not run `scripts/next-up.sh`, and do
not re-rank, re-filter or re-sort what you were handed.

Branch on which input you were handed: a prompt carrying `ranked` is the runner arm — go
straight to writing the answer; anything else is the interactive one, and you run the
script yourself. Both arms end in the identical ranked JSON.

**No credential at all?** `HB_SWEEP_FIXTURE=client/next-up/tests/fixtures/sweep.json` on
the `survey` verb short-circuits the fetch and exercises the whole path.

## What comes back

```jsonc
{
  "candidates": [ { "item": Item, "reasons": [ReasonCode, ...] }, ... ],  // best first
  "health": {
    "triage": 2, "grilling": 0, "blocked_dropped": 1,
    "fog_exhausted": [ {"project_id": "...", "project": "...", "questions": ["...", ...]} ]
  }
}
```

`candidates` is already ranked and already filtered — archived, `Done`, `Blocked`-stage,
ungroomed-and-undated, and blocked-upstream items are gone before you see it. Do not
re-filter or re-sort it.

`reasons` is the reason-code vocabulary from `rank.rs`: `overdue`, `due_today`,
`in_progress_bias`, `priority`, `energy_match`, `size_fits`, `quick_before_next_start`,
`oldest_first`.

## Invocation and axis parsing

One parser handles every entry point. Every axis is independently skippable, and skipping
all three is fine and common — rank everything that qualifies.

- **Filter args**, any subset in any order: `office low 30m`, `@computer quick`, `high`.
  Contexts match with or without the `@`. Time is either a duration (`30m`, `2h`,
  `15 min`) or a size word; map a duration onto the nearest size — ≲15 min → `quick`, up
  to about an hour → `short`, beyond that → `deep`.
- **Free text** ("I've got twenty minutes and no brain") is a fast path: read it into the
  same three axes and proceed. Don't interrogate.
- **Bare `/next-up-hb`** → ask context, energy and time via the in-session multiple-choice
  prompt, each axis skippable ("anywhere" / "either" / "any"). One prompt, three axes if
  the harness allows it; plain text if it doesn't.

`--energy` is `low|medium|high` and `--size` is `quick|short|deep` — the owned schema's own
spellings, which the script rejects anything else for.

**No standalone GUI.** Deferred to v2 behind the dashboard tripwire — don't smuggle one in.

## Calendar context (optional input)

An optional add-on, never a fifth entry point. Its shape is issue #70's provider-agnostic
read contract, verbatim: `{current_or_next: {status, event}, today: [EventRecord, ...]}`,
where `status` is `in_progress | upcoming | none` and `event` is `null` iff the status is
`none`. Pass it with `--calendar <file>`, or as the envelope's `calendar` key on the runner
arm.

It arrives **from the device's mirror** (ADR-0005). This skill never calls a calendar
provider, never holds a calendar credential, and never polls.

**Absent → no display line and no ranking change.** That is the default.

**Present → one display line, before the top pick:**

- `in_progress` → `Now: <title> (until <end local time>)`
- `upcoming` → `Next: <title> at <start local time>`
- `none` → omit the line entirely (nothing to show, not an error)

The 30-minute nudge itself is `rank.rs`'s, applied for you — including the masked lookup
when an in-progress or all-day event hides the next start. You will see it as a
`quick_before_next_start` reason code, nothing more to compute.

## The *why* line

One line, naming the **actual** decisive rule, built from the winning candidate's own
`reasons` — "overdue since Saturday", "already in progress and fits @computer", "the only
quick thing left before your 11:00".

`oldest_first` rides on **every** candidate (step 6 always runs), so it is never on its own
evidence that age was decisive. Cite it only when no earlier reason separates the pick from
its runner-up — i.e. when the two candidates' reason lists agree up to that point.

## The fog reading

`health.fog_exhausted` hands you each project whose actions are all shut while fog rows are
still open, with every open `question` **verbatim**. The crate makes no judgment about
them, deliberately.

**Deciding whether a question names a real unknown is yours, and it is a reading, not a
regex.** "Update Acumatica"'s fog reading `None — the unknowns are carried inside the two
investigation actions` is a non-empty question that **must not flag**. A question that only
says "none", "n/a", or points at already-minted actions is not fog.

When one does name a real unknown, one footer line:

> ‹project›: route ends in fog, run `/to-actions`.

The selector **never mints**. It points at `/to-actions`.

## Output

Three parts, nothing more — plus the optional calendar line first.

1. **One top pick** — id (and `seq` when the item has one), title, and the one-line why.
2. **3–5 alternates** — one line each: id + title + the label or date that matters.
3. **A one-line health footer** — `Triage N · Grilling N`, plus "N more blocked upstream"
   when `blocked_dropped` is non-zero, plus any fog-exhausted project.

Never print the full list.

## The /microtask offer

When the user hesitates on the top pick, or the pick is a known-heavy `deep` chore, offer
`/microtask` — **one line, no obligation** — and drop it if they don't bite. Don't break
the item down here.

## Failure modes

- **Missing token** — the script prints one provisioning line and exits non-zero. Relay
  that line and stop. No stack trace, no retry, no offer to mint one.
- **A non-200 from the authority** — the script says which status came back. Report it and
  stop; do not fall back to a stale or invented survey.
- **Empty `candidates`** — don't invent a pick. Say nothing qualified and name what is
  actually there: the Triage/Grilling counts, the blocked-upstream count, or `/to-actions`
  if a project's route has run out.
- **A malformed calendar block** — the binary names what was wrong (a status claiming an
  event that isn't there, or the reverse) and exits non-zero. Fix the input or drop the
  calendar block; never guess at it.

## Out of scope

- **Delegation.** The owned schema cannot express it: `items` has no labels column, there
  is no labels table and no comments table, so all three legs of #10's protocol — the
  `agent` axis, the findings comment, and the `unlabel` that stops the re-offer loop — are
  missing. `/next-up-personal`'s delegation branch stays where it is; this skill does not
  reimplement it against a schema that cannot hold it. #291 tracks the owned-schema
  delegation marker that would let it come back.
- **Any write at all.** v1 is read-only: one `GET`, no `POST`, no `PATCH`, no minting, no
  re-labelling to "fix" tagging.
- **Calendar polling, API calls and credentials** — the skill only reads the context field
  it is handed.
- **A standalone read-GUI** — v2, behind the dashboard tripwire.
- **Batch orchestration** — permanently out (#10). One item at a time.
- **Linear, in any form.** No `ION-\d+` ids, no `linear.sh`, no workspace reads.
