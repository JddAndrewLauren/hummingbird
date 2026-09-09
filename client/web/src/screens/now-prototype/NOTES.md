# Drag a card between the frontier board's columns — the prototype

Issue **#801**. Throwaway: this whole directory is deleted before the gesture
is built for real, and whatever it decides survives in **ADR-0021's**
amendment. That is the #405 precedent — the board itself was chosen this way,
and the *rejected* approaches are the expensive part of what the ADR records.

## The question

> What does moving a card between columns **feel** like, and which affordance
> is right — a drag, a drag with a preview, a pick-then-place, or a thrown
> object?

Not "does the write work". The write path (`onTriage(itemId, null, edits)`)
already exists and is one call; nothing here reaches the worker. A drop writes
an in-memory override applied to the items **before** `groupFrontier`, so a
card moves through the real wasm grouping and the real lane packing, and a
reload forgets everything.

## Running it

```
cd client/web && pnpm dev
```

Then open `?demo&variant=A` — on the WSL IP, not `localhost`. `<` `>` in the
floating bar or the `←`/`→` arrow keys cycle the variants; in D, `1`/`2`/`3`
switch the physics mode. The bar prints the last move as
`<title> -> <axis>: <field> = <value>` after every drop, and `Reset` clears
every override.

Bare `?demo` — the URL `visual/surfaces.spec.ts` shoots — renders exactly
what it renders today. The gate is `import.meta.env.DEV` + the board demo
world + an explicit `variant` param.

## What is being compared

| | Primary affordance | Target | Touch | Keyboard | Motion |
| --- | --- | --- | --- | --- | --- |
| **A** | native HTML5 drag | the whole column | no | no | none |
| **B** | pointer drag | the header strip only | yes | no | the slot opening, `--dur-base` |
| **C** | pick, then place | a `Move here` button | yes | yes | none |
| **D** | physics | mode-dependent | yes | no | a spring, unbounded |

Common to all four, so the comparison is about the gesture and nothing else:
the field a drop writes (`drop-edits.ts`), and what a lit column looks like
(`styles.ts`). The highlight is always on the **column**, never on the card —
ADR-0021 decision 2 keeps a card's own colour meaning urgency. A refused
column (`overdue`, the one column no drop may land in) is never painted red;
it simply never lights.

### A — native drag, the whole column is the target

The cheap baseline, and the shape Phase 2 currently assumes. No dependency, no
custom motion, the browser's own ghost image. **Cannot work on touch at all** —
HTML5 DnD does not fire for a finger.

### B — pointer drag with a landing preview

Two disagreements with A: pointer events (so a finger works), and the header
strip alone as the target rather than the whole column. Instead of tinting the
destination, the destination **opens a slot** — the cards push down and an
accent-bordered space appears where the card is about to be.

### C — pick, then place

No drag. `m` (or a long press) picks a card up; every column that would accept
it lights and grows a `Move here` button; Escape puts it down. The only
variant native to every input the product has, and the only one a screen
reader gets for free. It is here to make the drag variants earn their
complexity.

### D — physics

The card is attached to the pointer **by a spring**, not to the pointer. It
lags, it tilts into its own travel, it overshoots and settles. Three modes,
one integrator:

1. **throw** — release with speed and it flies, decelerates under friction,
   and lands in whichever column its *projected* resting point falls in. The
   aim is taken before the flight starts, so flight and settle are one spring.
2. **weighted** — same weight and settle, no ballistics: release over the
   target. The calm reading.
3. **magnetic** — the card tracks the pointer until a column's pull takes it
   off the cursor and into the slot, then resists leaving.

Refusal is physical too: a card thrown at `overdue`, or at nothing, springs
home. After a legal drop one FLIP frame carries the card from where it landed
to where the re-grouped board actually put it, so the throw reads as
continuous.

**D deliberately contradicts the design system.** Motion there is 90–320ms,
"nothing drifts or floats", and overshoot is allowed on hover and nowhere
else. A thrown card drifts by definition. If D wins, that is an argument to
**amend the motion section**, and the amendment belongs in Phase 2 rather than
being smuggled in as a detail. Under `prefers-reduced-motion` the engine does
not run at all.

## Known fidelity limits (prototype-grade, not findings)

- B's landing slot opens under the target column's **header**, not at the
  index `orderFrontier` will actually put the card at. The preview says which
  column, not which position.
- C's long press and B/D's drag both suppress the click that would open the
  item panel. A real build has to decide that interaction properly.
- The board is not scrolled by a drag near the window edge (A gets the
  browser's auto-scroll free; B and D do not).
- Column geometry is frozen at gesture start, so a board that reflows
  mid-gesture (a window resize) hit-tests against stale rects.

## Verdicts

Operator, 2026-09-08, after flipping all four.

- **A** —
- **B** —
- **C** —
- **D**, throw —
- **D**, weighted — **the winner.**
- **D**, magnetic —

**Winner:** **D mode 2, weighted.** The card hangs off the pointer on a
spring — it lags, tilts into its own travel, overshoots and settles — but it
is *not* thrown: it is released over its target, and the landing is the
pointer's, not a projection's. So the variant keeps the weight and drops the
ballistics.

Consequences worth carrying into the build:

- It is a **pointer-event** gesture, not HTML5 DnD. That reverses the Phase 2
  plan's "native DnD, no touch" assumption, and everything A got free — the
  ghost image, the drop cursor, edge auto-scroll — has to be built.
- The **whole column** is the target (weighted hit-tests the pointer against
  the frozen column rects), so B's header-strip narrowing is rejected with B.
- The design system's motion section still has to be **amended** or the
  spring constrained: a settle that overshoots is outside "nothing drifts or
  floats", and this is the mode that keeps the overshoot while dropping the
  drift. That amendment belongs in the ADR alongside the gesture.
- Refusal stays physical: a card released over `overdue`, or over nothing,
  springs home. Nothing is painted red.

**What this overturns in the Phase 2 plan** (it currently assumes: native DnD,
the whole column as the target, a tint rather than a placeholder, `overdue`
refused rather than hidden, and no touch support) —
