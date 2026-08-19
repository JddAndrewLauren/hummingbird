---
name: microtask
description: Break one already-selected, stalled hummingbird item into a checklist of ~2–5-minute concrete physical steps, written as Step records against the app-owned authority, lowering activation energy on it. Use when John says an item is too big to start, asks to break something down, or a picked item has stalled. Not a planning tool and not a selector.
---

# Microtask (OpenClaw arm)

Lower activation energy on **one already-selected, stalled item** by writing
a checklist of tiny concrete steps. Write first, offer company second. Speed
is the value.

This is the OpenClaw interactive arm of the hummingbird repo's `/microtask`
skill (ADR-0029) — a sibling of its Claude-session arm and its hosted-runner
arm (#272). `scripts/hb.sh` here is a **verbatim copy** of that skill's own
script, CI-pinned against drift: the deterministic step id
(`sha256(namespace + item + "/" + body)`, frozen namespace
`hummingbird-skill/microtask/v1`) is what keeps the three arms from ever
minting two copies of one step. Never edit the recipe here; it changes in
the hummingbird repo or not at all.

All reads and writes go through `scripts/hb.sh`:

- `{baseDir}/scripts/hb.sh get <ref>` — the item and its live steps, in position order
- `{baseDir}/scripts/hb.sh steps <ref>` — just the live steps
- `{baseDir}/scripts/hb.sh add-step <ref> <body>` — one step, appended
- `{baseDir}/scripts/hb.sh add-steps <ref> <file>` — one step per non-blank line, appended
- `{baseDir}/scripts/hb.sh tick <step-id>` — `{done: true}`
- `{baseDir}/scripts/hb.sh drop-step <step-id>` — `{deleted_at: now}`; flagged, never erased

`<ref>` is `HB-42` (case-insensitive) or a bare uuid.

## Steps are records, not markdown

Steps are rows in a `steps` table with their own `position`, `done` and
`version`; every write is a scalar CAS on one row. Do not reintroduce any
markdown-era machinery: no markers, no read-modify-write merge over a body
string, no checkbox parsing.

## Invocation

On an item John names or that is already in conversation (often straight off
`hummingbird-tasks` sweep context). Resolve it with `hb.sh get` immediately.
An optional grain `1`–`3` (default **2**) sets how finely to slice.

## Grain

1. **Coarse** — each step one concrete action; a gather may ride along with
   its use. A message-sending action ≈ 8 steps.
2. **Default** — every gather, compose, and send is its own tick; no step
   bundles two moves (locate ≠ copy ≠ paste, compose ≠ send). ≈ 15 steps.
3. **Max** — one physical/mental move per tick. ≈ 23 steps. The floor:
   individual keystrokes never get ticks.

The trivial first step survives every grain.

## Read first, ask at most once

Read the item's title, description, project and axes. Ask **one** question
only if they give nothing to work with — "what's in the way?" or "what does
done look like here?". Never a second.

## The live-plan rule (#307/#312, carried into this arm)

`get` the item before writing anything. If it already has live **unticked**
steps, that is a live plan: **never append a second plan on top of it
bare.** Report the existing steps (ticked = record, unticked = plan) and ask
John whether to continue walking the existing plan or rewrite it. Only on an
explicit rewrite: `drop-step` each superseded unticked step (the row stays,
flagged), then `add-steps` the new lines. "This step no longer applies" is a
reading of the work — yours to make in conversation, never silent.

An item whose live steps are all `done` has no plan to protect: append after
them, and **report the already-done steps first** — never silently bury
progress.

## Write the steps

Write the checklist to a file, one step per line, and hand it to
`hb.sh add-steps`.

- Each step is **one concrete physical action, ~2–5 minutes** (grains 1–2).
- **First step deliberately trivial** — the ramp.
- Anti-patterns, never do these: sub-items in any form; multi-item
  breakdowns; planning-shaped steps ("decide what to keep" is fog, not a
  step); re-ranking or selection.

Re-running the identical checklist mints nothing: the deterministic id lands
a replay on the already-exists path, which is what makes an interrupted
write safe to simply repeat.

## Walk-through mode

Offer it **only after** the checklist is written, never before. On accept:
John reports a step done → `hb.sh tick <step-id>` → hand over the next step.
A step he ticked in the app is already `done` when you get there — that
agreement between surfaces is the point of Steps being records.

## Failure modes

- **Missing token** — the script prints one provisioning line and exits
  non-zero. Relay that line and stop. No retry, no offer to mint one.
- **A non-200 from the authority** — the script says which status came
  back. Report it and stop.
- **A 409 the script could not settle** — say which step, and stop.
- **An unknown ref** — a named failure, not an empty answer. Do not fall
  back to writing steps against something else.
- **Scope guard** — this script writes only `steps` rows. Item edits go
  through the `hummingbird-tasks` skill; do not reach for another skill's
  script to get around either guard.
