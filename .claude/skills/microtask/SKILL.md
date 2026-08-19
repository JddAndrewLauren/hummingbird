---
name: microtask
description: Break one already-selected, stalled item into a checklist of ~2–5-minute concrete physical steps, written as Step records against the app-owned authority (ADR-0008/0009), lowering activation energy on it. Use when the user invokes /microtask, says an item is "too big to start", asks to "break this down", or a picked item has stalled. Not a planning tool — /to-actions does decomposition, /next-up-hb does selection.
---

# Microtask

Lower activation energy on **one already-selected, stalled item** by writing a checklist of
tiny concrete steps. Write first, offer company second. Speed is the value.

Vocabulary (Action, **Step**, Route, Fog) is in the root `CONTEXT.md`.

## The two arms (of this file — a third, OpenClaw's, lives at `openclaw/microtask/`)

**Interactive** (a session, with the operator's credential): you read and write through
`scripts/hb.sh` yourself, and everything below applies as written. (The OpenClaw arm,
ADR-0029, is the same posture under its own SKILL.md, with a verbatim, CI-pinned copy of
`scripts/hb.sh` — edits to the script happen here and are re-copied there.)

**Hosted runner** (#272's third op; #307's decline; #317's rewrite): the runner has
already read the item's live steps from the authority. A bare run only ever reaches you
when every one of them is already `done` — an item with any live *unticked* step is
declined by the runner itself, before you are called, naming the count and the remedy,
and a different `grain` does not change that; the remedy is `replace: true`. So whatever
steps ride in your prompt are always `record`, never a plan to continue — that is true on
a bare run and on a replace alike, since the runner never shows you the plan it may be
about to rewrite: report them in `note` and never re-propose them. You have **no shell**
here, so do not run `hb.sh`, and asking for it wastes the run (`claude -p` is
non-interactive, a `Bash` call cannot be prompted for and is simply denied). Answer
against `schema.json` — `{steps, note}` — and the runner does the reconciling: on a bare
run it appends those lines as the plan, with the same deterministic ids; on `replace:
true` it diffs your answer against the live unticked steps by exact text — one your
answer repeats verbatim is kept and repositioned, one that is absent is soft-deleted,
everything else is created — and ticked steps are never touched. You never see or emit a
step id either way. It also cannot ask a question — say what you assumed instead.

Branch on which input you were handed: a prompt carrying the item and steps as JSON is the
runner arm; anything else is the interactive one.

All reads and writes on the interactive arm go through `scripts/hb.sh` (in this skill's
directory):

- `hb.sh get <ref>` — the item and its live steps, in position order
- `hb.sh steps <ref>` — just the live steps
- `hb.sh add-step <ref> <body>` — one step, appended
- `hb.sh add-steps <ref> <file>` — one step per non-blank line, appended
- `hb.sh tick <step-id>` — `{done: true}`
- `hb.sh drop-step <step-id>` — `{deleted_at: now}`; flagged, never erased

`<ref>` is `HB-42` (case-insensitive) or a bare uuid.

## Steps are records, not markdown

This is the whole difference from the Linear era, and it deletes three things rather than
porting them. **Do not reintroduce any of them:**

- **No `<!-- microtask:start -->` markers.** Steps are rows in a `steps` table with their
  own `position`, `done` and `version`.
- **No read-modify-write merge.** There is no shared string for a concurrent editor to
  clobber. Each step is its own row, and each write is a scalar CAS on that row —
  literally the operation `server/domain/src/step.rs` names as "the operation whose
  impossibility under Linear triggered ADR-0008".
- **No `- [x]`/`- [X]` normalisation.** `done` is a boolean column. There is no casing to
  get wrong and no checkbox to parse.

All three existed only because the body was one opaque string two parties wrote to.

## Invocation

`/microtask <ref> [grain]` (e.g. `HB-10`, `HB-10 3`), or on an item already in
conversation context (handed off from `/next-up-hb` or `/to-actions`). Resolve it with
`hb.sh get` immediately. `grain` is an optional `1`–`3` (default **2**) — see Grain.

## Grain

How finely to slice, calibrated on a real trial:

1. **Coarse** — each step one concrete action, but a gather may ride along with its use
   ("find the environment URL and copy it"). A message-sending action ≈ 8 steps.
2. **Default** — every gather, compose, and send is its own tick; no step bundles two
   moves (locate ≠ copy ≠ paste, compose ≠ send). The same action ≈ 15 steps. This split
   kills the stall where you open the chat and realize you don't have the link handy.
3. **Max** — one physical/mental move per tick ("click the search bar and type his
   name", "hit send"). The same action ≈ 23 steps. The floor: individual keystrokes
   never get ticks.

At grain 3, steps run well under the 2-minute floor — that's the point; the ~2–5-minute
guidance applies at grains 1–2. The trivial first step survives every grain.

On the runner arm the invocation is
`POST /run {skill: "microtask", args: {ref, grain?, replace?, model?}}`, and `ref` is
resolved before you are called — an unknown one never reaches you. `model` picks which
model runs you (#273); it never reaches your prompt, and the answer's schema is the same
whichever one it names.

## Read first, ask at most once

Read the item's title, description, project and axes. Ask **one** question only if they
give nothing to work with — "what's in the way?" or "what does done look like here?".
Never a second: three questions in, the user would rather just do the chore.

## Write the steps

Write the checklist to a file, one step per line, and hand it to `hb.sh add-steps` — or, on
the runner arm, return those same lines as `steps` and let the runner append them.

- Each step is **one concrete physical action, ~2–5 minutes**. Minutes, not sittings.
- **First step deliberately trivial** ("put on music, grab a trash bag") — the ramp.
- Anti-patterns, never do these: sub-items in any form; multi-item breakdowns;
  planning-shaped steps ("decide what to keep" is fog, not a step); re-ranking or
  selection.

Steps are appended after whatever is already there, and `add-steps` numbers the batch
contiguously from the current maximum position. **Re-running the identical checklist mints
nothing**: each step's id is derived deterministically from the item and the step's own
text, so a replay lands on the idempotent already-exists path. That is what makes an
interrupted write safe to simply repeat. The runner arm mints the identical id from the
identical inputs (`runner/src/step-id.js`), so the two arms cannot mint two copies of one
step between them.

## Refresh rule

On a re-run, `get` the item first and **report the already-`done` steps to the user before
adding more** — never silently bury progress.

This survives from the Linear era in substance and is much smaller in form: it is now a
field read (`done`) rather than a case-insensitive regex over prose, so there is no
normalisation rule attached to it and no way to miss a tick that the client made.

Then decide what has been superseded. **That decision is yours and stays here in prose,
on the interactive arm** — `hb.sh` deliberately does not reconcile a checklist for you,
because "this step no longer applies" is a reading of the work, not a diff. Soft-delete
each superseded step with `drop-step` (the row stays, flagged) and `add-steps` the
genuinely new ones, which land at higher positions.

**On the runner arm this decision is never the model's** (#317): `replace: true` hands the
runner your answer and the runner itself computes what has been superseded, by comparing
text against the live unticked steps — never by asking you to judge, since you never see
that plan. What stays banned there is the model deciding *per-step* what has been
superseded; a caller-directed, wholesale replacement is a different thing.

## Walk-through mode

**Interactive arm only** — the runner arm is one-shot and has nothing to walk with.

Offer it **only after** the checklist is written, never before. On accept: the user reports
a step done → `hb.sh tick <step-id>` → hand over the next step. Declining costs nothing —
the checklist is already persisted and usable from the client.

`tick` is idempotent, and that matters more than it sounds: **a step the user ticked in the
client is already `done` when you get there**, because every read here is a fresh sweep.
That agreement between the two surfaces is the point of Steps being records.

## Failure modes

- **Missing token** — the script prints one provisioning line and exits non-zero. Relay
  that line and stop. No stack trace, no retry, no offer to mint one.
- **A non-200 from the authority** — the script says which status came back. Report it and
  stop.
- **A 409 the script could not settle** — disjoint touched fields are retried once, an
  already-applied value is accepted, and a divergent touched field stops with its name.
  Say which step, and stop.
- **An unknown ref** — `HB-99` that is not in the sweep is a named failure, not an empty
  answer. Do not fall back to writing steps against something else.
- **On the runner arm none of the above is yours to report.** Every authority failure —
  no token, an unreachable server, a non-200 on a write — happens outside your run and
  ends the stream in a named `{ok:false, error}` envelope. Your only failure mode there is
  answering outside the schema.
- **Scope guard** — write only `steps` rows. This skill's `hb.sh` has no verb that touches
  an item, a project, a route or the delegation axis, so the guard is structural; do not
  reach for another skill's script to get around it.
