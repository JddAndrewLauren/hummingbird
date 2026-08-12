// Which gesture an item's own steps make legal (#273, against #307).
//
// The seam declines a bare run against an item carrying a live *undone*
// plan, and rewriting takes an explicit `replace: true`. The client reads
// the item's steps through the normal path anyway, so it knows which
// gesture is legal before the tap and offers only that one — the seam's
// decline stays a backstop for races and non-client callers, and #307's
// body says not to string-match its prose to pick an affordance.

import type { StepDTO } from "../store/protocol";

/**
 * The steps that still have a plan left in them — `undoneSteps` over
 * `liveSteps` in `runner/src/skills/microtask.js`, the same two predicates
 * in the same order.
 *
 * Soft-deleted rows are history, and a `done` step is *record*: neither is
 * something a continuation would clobber.
 */
export function liveUndoneSteps(steps: StepDTO[]): StepDTO[] {
  return steps.filter((step) => step.deletedAt === null && !step.done);
}

export type MicrotaskAffordance =
  | { kind: "break" }
  | { kind: "rewrite"; undoneCount: number };

/**
 * **All-done is `break`, not `rewrite`** (#307 point 1). Ticked steps are
 * record rather than plan, so an item whose live steps are all done has
 * nothing left to protect and an append after them is the normal case — the
 * seam agrees, and offering "Rewrite 0 steps" there would both read as
 * nonsense and send a `replace: true` that has nothing to replace.
 */
export function microtaskAffordance(steps: StepDTO[]): MicrotaskAffordance {
  const undone = liveUndoneSteps(steps);
  return undone.length === 0 ? { kind: "break" } : { kind: "rewrite", undoneCount: undone.length };
}
