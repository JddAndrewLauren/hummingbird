// Whether a sync outcome says anything about how stale the mirror is
// (#535) — split out of `sync-status.ts` on purpose.
//
// `hummingbird_core::decisions::settings::is_informative_sync_outcome` is
// the real rule and `sync-status.ts`'s other functions reach it through
// `decisions/seam.ts` — but `worker/ports.ts` needs this exact predicate
// too, and it runs inside `core.worker.ts`'s own script evaluation
// (ADR-0010). The seam is a SECOND, main-thread wasm instantiation
// (`decisions/seam.ts`'s own doc); a static import of it from anything
// reachable in the worker's graph would instantiate that second module
// during the worker's evaluation, which `worker-import-graph.test.ts`
// gates against. Since ES imports pull in a module's WHOLE static import
// graph regardless of which export is used, this predicate has to live
// somewhere `sync-status.ts` (which does import the seam) is not — this
// file, imported directly by both `worker/ports.ts` and `sync-status.ts`.
//
// Deliberately NOT sunk at runtime for that structural reason, the same
// "module-evaluation/graph-boundary" carve-out ADR-0025's verdict table
// already draws for `field-vocabulary.ts`'s arrays — pinned against the
// core's real answer by this file's own test instead.

import type { TaskRunOutcomeKind } from "../store/protocol";

/** `RunOutcome::kind`s that mean "nothing was attempted" — `Skipped`'s
 * backoff not being ready yet, or a host wedged mid-cycle. Every other
 * kind says something about staleness. */
const NOT_RUN: readonly TaskRunOutcomeKind[] = ["skipped", "busy"];

export function isInformativeSyncOutcome(kind: TaskRunOutcomeKind): boolean {
  return !NOT_RUN.includes(kind);
}
