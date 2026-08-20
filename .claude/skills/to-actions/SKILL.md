---
name: to-actions
description: Break a personal project into actions against the app-owned authority (ADR-0008/0009) via a bounded mini-grilling interview, minting items in one confirmed batch with native blocked_by edges and a Route held as first-class records. Use when the user wants to break down a personal project into actions, plan a project, invokes /to-actions, or asks "what are the actions for X".
---

# /to-actions

Break a personal **project** into **actions** via a bounded interview. The personal
analogue of `/to-tickets`: publish the graph, work the frontier — with **fog held back**
as its own records instead of forced into premature actions.

Vocabulary (Action, Route, Destination, Fog, Mint, External wait) is in the root
`CONTEXT.md`. Read it before starting. API mechanics are in [REFERENCE.md](REFERENCE.md).

All reads and writes go through `scripts/hb.sh` (in this skill's directory):

- `hb.sh project-find <name>` — the project, its Route, its fog and its actions
- `hb.sh project-create <name>` — and its Route row, created with it
- `hb.sh route-set <project-ref> [--destination <file>] [--notes <file>]`
- `hb.sh fog-add <project-ref> <question>` / `hb.sh fog-resolve <fog-id>`
- `hb.sh mint <manifest-file>` — the one confirmed batch
- `hb.sh block <ref> <blocker-ref>` — *ref* is blocked by *blocker-ref*
- `hb.sh archive <ref>` — cancel an action by setting `archived_at`

## Preflight

1. `hb.sh project-find <name>`. Missing token → the script prints one provisioning line
   and exits non-zero; relay that line and stop.
2. Three cases:
   - **Exists** → this is a re-run; read its Route first (see Re-runs).
   - **Doesn't exist** → ask for an explicit yes before `project-create`. Never create a
     project without one (workspace-mutation rule).
   - **The target is a standalone task, not a project** → say "this looks like a single
     action, not a project" and suggest minting it directly instead. Don't force a
     breakdown.

## The interview

Bounded by three artifacts, not by thoroughness. Ask **one question at a time**; budget
is "a handful" of questions total.

1. **Destination** — what done looks like, in the user's own terms.
2. **Route** — what has to happen, roughly in what order, and where the user's knowledge
   runs out. Fog is explicitly allowed and expected — record each foggy segment with the
   open question that blocks defining it.
3. **Definition test** per candidate segment — "could you start this with no more
   decisions? what does done look like?" Startable within one `size` (`deep` is the
   ceiling) → action. Bigger → sub-route. Needs more decisions → fog. (A segment that's
   fully defined but waiting on an upstream action is *not* fog — it gets minted with a
   `blocked_by` edge.)

Exit when every definable segment is captured and the Route is written. If the project
itself is foggy enough to need real stress-testing, say so and punt to `/grilling` (or the
Grilling stage) — do **not** impersonate a full grilling session.

## The Route is records now

`## Destination` is `routes.destination`, `## Notes` is `routes.notes`, `## Fog` is `fog`
rows, `## Actions` is items carrying `project_id` ordered by `project_pos`. **The
four-section markdown template is gone, not reimplemented over a description field** —
Routes being first-class is one of the two things that triggered ADR-0008.

Two things follow that are worth stating rather than discovering:

- **There is no "rewrite the whole Route" call, and no need for one.** Each part is its
  own row, so editing Notes cannot lose Fog and refreshing the action list cannot lose
  Destination. Set what changed; leave the rest alone.
- **An open fog row *is* fog.** The old rule that "the fog check is a reading, not a
  regex" existed because a `## Fog` section might say "None — the unknowns are carried
  inside the two investigation actions". A row with a `resolved_at` needs no such reading:
  resolve a question when it is answered, and never write a row saying there is no fog.
- **This skill is not the Route's only writer.** Since ADR-0030 the human edits
  destination, notes, fog and action order from the app too, and the ordinary
  compare-and-set arbitrates — a 409 on a re-run means someone edited that row, so re-read
  and rebase rather than treating it as an error.

Actions are listed by `project_pos` — orientation only; the `blocked_by` edges are the
real sequencing.

## Minting

- Mint **every definable segment** — startable or not — in **one confirmed batch**:
  preview all titles, proposed axes, the `blocked_by` edges and the Route changes, get one
  confirm/correct pass, then write. No per-action prompting — everything the run will
  write is in that one preview.
- Proposed axes: `context` (`@home`, `@computer`, …) is usually inferrable; `size` and
  `energy` are guesses the human corrects — say so in the preview. The spellings are
  `quick`/`normal`/`deep` and `low`/`medium`/`high` (REFERENCE.md; both changed from the
  Linear vocabulary).
- All minted actions land in **`ready`**. **Never propose `agent` by default** — the
  delegation axis is deliberate, and `/next-up-hb` is where a marked item gets handed off.
- Sequencing is `blocked_by` edges between actions. That is the whole hand-off: closing an
  action frees its dependents. No minting step, no session, no daemon.
- **`stage: blocked` is never written for an inter-action dependency** — it stays reserved
  for genuinely external waits (a callback, a part in the mail).
- Route changes later → **cancel-and-remint** the affected actions. Never rewire edges in
  place. Run `hb.sh archive <ref>` for each affected action. Cancel is `archived_at`; the
  owned schema has no Canceled stage.

**A failed batch is re-run, not repaired.** Every id is settled before the first write, so
re-running the same manifest replays identically: the already-minted half answers with its
stored row and nothing is duplicated. Do not hand-reconcile a partial batch, and do not
edit the manifest to "skip the ones that worked" — that is how a re-run mints a second
copy of everything you changed.

## Re-runs

On an existing project: `project-find` first. Skip interview ground already answered
(Destination and Notes stand unless the user reopens them); revisit only the open fog
entries — has any question been answered? `fog-resolve` those, mint the newly-definable
segments (one confirmed batch, same rules), and `route-set` whatever actually changed.

## Hand-off

End by offering `/microtask` on a stall-prone first action — an offer, not a merger.

## Failure modes

- **Missing token** — one provisioning line from the script, relayed verbatim. Stop.
- **A non-200 from the authority** — the script says which status and which action it was
  posting. Report it and stop; do not retry the batch blindly, `mint` again is the
  supported move and it is safe.
- **A 409 the script could not settle** — disjoint touched fields are retried once, an
  already-applied value is accepted, and a divergent touched field stops with its name.
  Say which row, and stop.
- **A 400 on a mint** — a closed vocabulary was violated (a `size` of `medium`, a
  `priority` outside `0..=4`, a `deadline` that is not `YYYY-MM-DD`/`YYYY-MM-DDTHH:MM`),
  or a server-stamped field was supplied. The message names it. Fix the manifest and
  re-run; nothing partial was left behind that a re-run will duplicate.

## Out of scope

Edge bookkeeping beyond create-time `blocked_by`; sweeper-assisted minting; sub-items; any
batch orchestration. **Also: writing the delegation axis, and touching `steps`** — those
belong to `/next-up-hb` and `/microtask`, and this skill's `hb.sh` has no verb for either.
