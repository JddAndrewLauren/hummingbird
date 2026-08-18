// Which microtask gesture an item's own steps make legal (#273, against
// #307). Sunk to `hummingbird_core::decisions::skills::microtask_affordance`
// at #539 (ADR-0025) — this module is now a thin wrapper over the seam, kept
// so every existing caller and test is unchanged.
//
// The seam declines a bare run against an item carrying a live *undone*
// plan, and rewriting takes an explicit `replace: true`. The client reads
// the item's steps through the normal path anyway, so it knows which
// gesture is legal before the tap and offers only that one — the seam's
// decline stays a backstop for races and non-client callers, and #307's
// body says not to string-match its prose to pick an affordance.

import { microtaskAffordanceFromCore } from "../decisions/seam";
import type { StepDTO } from "../store/protocol";

/**
 * The steps that still have a plan left in them — live (not soft-deleted)
 * and not done. Soft-deleted rows are history, and a `done` step is
 * *record*: neither is something a continuation would clobber.
 *
 * Kept as plain TS (not sunk): it is a one-line filter with no client
 * disagreement possible, and `grill-review.ts`'s own predicates now read
 * the full step list directly through the seam rather than through this
 * helper.
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
 *
 * The rule itself is `hummingbird_core::decisions::skills::
 * microtask_affordance`; this is the seam call, and nothing else.
 */
export function microtaskAffordance(steps: StepDTO[]): MicrotaskAffordance {
  return microtaskAffordanceFromCore(steps) as MicrotaskAffordance;
}
