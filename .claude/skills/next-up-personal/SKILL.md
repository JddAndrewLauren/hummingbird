---
name: next-up-personal
description: Pick what to do right now from the personal Linear workspace (org twinion, team ION) — survey Ready/In Progress work, rank it by context, energy and time, and present one top pick with a few alternates and a pipeline-health footer; also the front door for handing a single `agent`-labelled issue to an agent. Use when the user asks "what should I do", "what's next", "what can I hand off", invokes /next-up-personal, or names an issue to delegate. Not a decomposer — /to-actions breaks a project into actions, /microtask breaks a picked issue into steps.
---

# /next-up-personal

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

**No standalone GUI.** Deferred to v2 behind the dashboard tripwire — don't smuggle one in.

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
6. **Oldest first** — `createdAt` ascending, so nothing quietly rots.

## Output

Three parts, nothing more:

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
