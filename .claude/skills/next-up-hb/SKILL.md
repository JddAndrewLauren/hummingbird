---
name: next-up-hb
description: Pick what to do right now from the app-owned authority (ADR-0008) — survey what is startable, rank it with the deterministic ranker, and present one top pick with a few alternates and a pipeline-health footer; also the front door for handing a single agent-marked item to an agent. Use when the user asks "what should I do", "what's next", "what can I do in twenty minutes", "what can I hand off", invokes /next-up-hb, or names an item to delegate. Not a decomposer — /to-actions breaks a project into actions, /microtask breaks a picked item into steps.
---

# /next-up-hb

> **Why the `-hb`:** the operator's profile ships a different `/next-up`
> (pick the next piece of work across registered projects). This is the
> hummingbird-specific one, and the suffix is what keeps the two slash
> commands unambiguous. The runner op name and this directory carry it too.

A **task selector**, not a router: answer "what should I do right now" against the
app-owned authority with **one** top pick. Plus one branch — handing a single chosen item
to an agent. Never a full list.

Vocabulary (Action, Route, Destination, Fog, Delegation axis, External wait) is in the
root `CONTEXT.md`.

**This skill talks to nothing but hummingbird's own API.** No Linear, no issue ids of any
other tracker, no `linear.sh`. `/next-up-personal` was the Linear-era skill and is
**retired** (#115) — this one replaced it, delegation branch included.

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
  [--agent] [--calendar <file>]
```

One read-only `GET /api/sweep`, then the ranker. Everything the survey needs — items,
blocked-by edges, projects, fog — arrives in that one payload. This arm also carries the
delegation verbs below.

**Hosted runner** (#41/#256's second op): the runner is **context-blind**. The sweep
payload arrives in the `{skill, args}` request from the calling device's mirror, so the
runner holds no authority token and makes no HTTP call. **It therefore cannot delegate**:
the branch below is three writes, and this arm has neither a credential nor a shell to
make them with. Report `agent_doable` and stop.

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
    "triage": 2, "grilling": 0, "blocked_dropped": 1, "agent_doable": 3,
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

- **An item ref** (`HB-12`, case-insensitive `HB-\d+`, or a bare uuid) → skip the survey
  entirely and go straight to **Delegation**. Required: delegation must work outside a
  next-up context.
- **Filter args**, any subset in any order: `office low 30m`, `@computer quick`, `high`.
  Contexts match with or without the `@`. Time is either a duration (`30m`, `2h`,
  `15 min`) or a size word; map a duration onto the nearest size — ≲15 min → `quick`, up
  to about an hour → `short`, beyond that → `deep`. Also **`agent`**, meaning "only what I
  could hand off" — that is `--agent`, the fourth axis, not one of the three.
- **Free text** ("I've got twenty minutes and no brain") is a fast path: read it into the
  same three axes and proceed. Don't interrogate.
- **Bare `/next-up-hb`** → ask context, energy and time via the in-session multiple-choice
  prompt, each axis skippable ("anywhere" / "either" / "any"). One prompt, three axes if
  the harness allows it; plain text if it doesn't.

`--energy` is `low|medium|high` and `--size` is `quick|short|deep` — the owned schema's own
spellings, which the script rejects anything else for.

**No standalone GUI.** Deferred to v2 behind the dashboard tripwire — don't smuggle one in.

## The fourth axis

Three axes rank; the fourth answers a different question. **Context, energy and size are
`rank.rs`'s** and are not restated here. `agent` — CONTEXT.md's **delegation axis**, *who
does this* — is the selector's, and it behaves unlike the other three in one way worth
holding:

**Context is a hard filter that untagged work survives. `agent` is a hard filter that
untagged work fails.** An item naming no context is doable anywhere; an item carrying no
`agent` is *the human's*. There is no marker for "the human does it" — the absence is the
marker. So `--agent` is the only axis that makes the answer smaller by default, which is
exactly what "what could I hand off right now" asks for: the 9pm case where the honest
answer isn't a smaller task but *not one of mine*.

Orthogonal to stage: an item can be agent-doable and still need grilling first.

The filter is applied by `client/next-up`'s selector, not by you — pass `--agent` and read
what comes back. `health.agent_doable` counts hand-off-able work on **every** survey,
marked or not, so you can make the offer below without a second call.

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
   when `blocked_dropped` is non-zero, plus "N you could hand off" when `agent_doable` is,
   plus any fog-exhausted project.

Never print the full list.

## Delegation branch

Reached two ways: `/next-up-hb <item-ref>`, or the user accepting the offer on a pick that
carries `agent`. Offer it on any `agent`-marked pick — one line, no push.

On a directly-named item, `next-up.sh get <ref>` it first and read what came back:

- `agent` is false → say so and ask for an explicit go-ahead before running it anyway;
- non-empty `blockers` → say what is blocking it and stop, unless the user overrides;
- already `done`, or archived → stop.

**Completion protocol — fixed by #10, do not re-decide it:**

1. **On start** — `next-up.sh move <ref> in_progress`. The visible claim, before any work.
2. **Do the work** — read the item, do the chore, produce something a human can act on in
   ten seconds (the three quotes and a recommendation, not a research diary).
3. **On finish** — in this order:
   - `next-up.sh note <ref> <file>` with the findings;
   - `next-up.sh move <ref> ready`;
   - `next-up.sh unflag-agent <ref>`.
4. **Genuine external blocker only** — waiting on a callback, a form needing a human
   identity, a credential you cannot hold: `note` what is needed, then
   `move <ref> blocked`. Not for "this was harder than expected", and never for a
   dependency on another item — that is a `blocked_by` edge, minted by `/to-actions`.

**Never `done`.** An agent chore *advances* a chore, it does not complete it: "compare
three insurance quotes" ends with three quotes and a recommendation; choosing and buying
is the human's. Closing would silently drop the remaining human step. The script refuses
`move <ref> done` outright, so this is enforced and not merely asked for.

**Always clear the axis on finish.** `agent` means there is agent work *left* here, not
that an agent touched this once. Leave it set and the next survey re-offers the hand-off
and the agent redoes its own research into a second near-identical findings block.

If a step fails mid-protocol, report exactly where it stopped — an item left In Progress
with no findings is worse than one never claimed. Every verb is idempotent (`move` to the
stage it already holds, a `note` that replaces its own section, `unflag-agent` on an
already-clear axis), so a re-run of a half-finished finish is safe.

**Where the findings actually go, and why you should say so if asked.** The owned schema
has no comments table, so `note` appends to the item's `description` under
`<!-- agent-findings -->` markers, replacing that section on a re-run. This is an
acknowledged stopgap recorded in ADR-0009's 2026-08-11 amendment, not the end state — a
real `notes` table is the follow-up. Two consequences for you: **do not hand-edit that
section** (use `note`, which owns it), and **do not put anything in it that belongs in the
item's own description** — the prose above the markers is the human's.

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
  if a project's route has run out. On an `--agent` survey, "nothing to hand off" is a
  perfectly good answer and the whole list is one line.
- **A 409 the script could not settle** — it retried once and the row moved again, so
  another writer is on it. Report *where in the protocol it stopped* and stop; do not
  re-run the whole protocol from the top, which would re-claim an item someone else is
  now moving.
- **`move <ref> done`** — refused by the script, with the reason. That is the protocol
  working, not an error to route around.
- **A malformed calendar block** — the binary names what was wrong (a status claiming an
  event that isn't there, or the reverse) and exits non-zero. Fix the input or drop the
  calendar block; never guess at it.

## Out of scope

- **Any write beyond the delegation protocol's three.** The selector reads; delegation
  writes exactly a stage, a findings section and the axis. No minting, no description
  edits outside the markers, no Route refresh, no re-marking to "fix" someone's tagging.
- **Delegation on the hosted runner arm.** Structural, not a rule: that arm holds no
  credential and has no shell. It reports `agent_doable` and stops.
- **Calendar polling, API calls and credentials** — the skill only reads the context field
  it is handed.
- **A standalone read-GUI** — v2, behind the dashboard tripwire.
- **Batch orchestration** — permanently out (#10). One item at a time.
- **Linear, in any form.** No other tracker's issue ids, no GraphQL, no workspace reads.
  `HB-<seq>` is the only handle vocabulary there is.
