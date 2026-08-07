---
name: to-actions
description: Break a personal project into actions on Linear (org twinion, team ION) via a bounded mini-grilling interview, minting issues in one confirmed batch with native blocked-by relations and a Route held in the project description. Use when the user wants to break down a personal project into actions, plan a project on Linear, invokes /to-actions, or asks "what are the actions for X".
---

# /to-actions

Break a personal **project** into **actions** on Linear via a bounded interview. The
personal analogue of `/to-tickets`: publish the graph, work the frontier — with **fog
held back** in the project description instead of forced into premature issues.

Vocabulary (Action, Route, Destination, Fog, Mint, External wait) is in the root
`CONTEXT.md`. Read it before starting. API mechanics are in [REFERENCE.md](REFERENCE.md).

## Preflight

1. Read `~/.config/linear/api-key`. If absent, say exactly one line —
   `Linear API key missing: put it in ~/.config/linear/api-key` — and stop.
2. Look up the Linear project (org `twinion`, team `ION`). Three cases:
   - **Exists** → this is a re-run; read its description first (see Re-runs).
   - **Doesn't exist** → ask for an explicit yes before creating it. Never create a
     project without one (workspace-mutation rule).
   - **The target is a standalone task, not a project** → say "this looks like a single
     action, not a project" and suggest minting it directly instead. Don't force a breakdown.

## The interview

Bounded by three artifacts, not by thoroughness. Ask **one question at a time**; budget
is "a handful" of questions total.

1. **Destination** — what done looks like, in the user's own terms.
2. **Route** — what has to happen, roughly in what order, and where the user's knowledge
   runs out. Fog is explicitly allowed and expected — record each foggy segment with the
   open question that blocks defining it.
3. **Definition test** per candidate segment — "could you start this with no more
   decisions? what does done look like?" Startable within one `size` label (`deep` is
   the ceiling) → action. Bigger → sub-route. Needs more decisions → fog. (A segment
   that's fully defined but waiting on an upstream action is *not* fog — it gets minted
   with a `blocked by` relation.)

Exit when every definable segment is captured and the Route is written. If the project
itself is foggy enough to need real stress-testing, say so and punt to `/grilling` (or
the Grilling state) — do **not** impersonate a full grilling session.

## Minting

- Mint **every definable segment** — startable or not — in **one confirmed batch**:
  preview all titles + proposed labels + the `blocked by` edges + the new/updated Route
  (project description), get one confirm/correct pass, then create. No per-issue
  prompting — everything the run will write is in that one preview.
- Proposed labels: context (`@home`, `@computer`, …) is usually inferrable; `energy` and
  `size` are guesses the human corrects — say so in the preview.
- All minted actions land in **Ready**. Never propose the `agent` label by default —
  delegation is deliberate.
- Sequencing = native Linear **`blocked by` relations** between actions. That is the
  whole hand-off: closing an action frees its dependents. No minting step, no session,
  no daemon.
- The **Blocked state is never written for inter-action dependencies** — it stays
  reserved for genuinely external waits (a callback, a part in the mail).
- Route changes later → **cancel-and-remint** the affected actions. Never rewire
  relations in place.

## The Route (project description)

Own a fixed four-section template in the Linear project description; refresh it on
every run:

```markdown
## Destination
<one or two lines, the user's terms>

## Fog
- <segment not yet definable> — open question: <what blocks defining it>

## Notes
- <constraints/decisions from the interview that later minting must respect>

## Actions
1. <minted issue title> (<identifier>)
```

Actions are listed in order, one line each — orientation only; the `blocked by`
relations are the real sequencing.

## Re-runs

On an existing project: read the Route first. Skip interview ground already answered
(Destination and Notes stand unless the user reopens them); revisit only the Fog
entries — has any open question been answered? Mint newly-definable segments (one
confirmed batch, same rules), then refresh the template.

## Hand-off

End by offering `/microtask` on a stall-prone first action — an offer, not a merger.

## Out of scope

Blocking-edge bookkeeping beyond create-time relations; sweeper-assisted minting;
sub-issues; any batch orchestration.
