// The routing decision #274 asks for: a pure function of (selection,
// registry, reachability memo, a passed-in clock reading) → which backend
// to attempt next. No `fetch`, no `Date.now()` — every dependency a test
// would otherwise have to fake is an argument instead, so `route-plan.test.ts`
// drives every branch with plain data.
//
// The one thing this module does NOT decide is whether an *attempt* itself
// succeeds — that needs a real network call, which is `route-run.ts`'s job.
// This module only ever answers "given what is already known, what should
// be tried, in what order, and is a pin dead enough to refuse outright."

import type { BackendEntry } from "./backend-registry";
import { AUTO_SELECTION, fallbackEntry, findBackendEntry } from "./backend-registry";
import { isFreshDead, type ReachabilityMemo } from "./reachability-memo";

/** One step of an Auto sequence. `skip` means "memoized dead — say so in the
 * narration, but do not spend a network round trip finding out again." */
export type RouteStep = { kind: "attempt"; entry: BackendEntry } | { kind: "skip"; entry: BackendEntry };

export type RoutePlan =
  /** Auto (or a pin with nothing memoized against it): try these, in
   * order. A pin's sequence is always exactly one step long — pinning
   * never falls through to another entry on its own. */
  | { kind: "sequence"; steps: RouteStep[] }
  /** A pin whose memo says it just failed. Declined without spending a
   * round trip on a backend already known dead — the loud decline #274
   * asks for, plus the one-tap fallback the picker can offer. */
  | { kind: "declined"; entry: BackendEntry; fallback: BackendEntry | null };

/**
 * `selection` is `AUTO_SELECTION` or a registered entry's id. An id that is
 * not (or no longer) in `registry` — a stale device-local preference after
 * a backend is retired — is treated exactly like Auto, so a picker's
 * leftover choice degrades to the safe default instead of resolving to
 * nothing.
 */
export function planRoute(
  selection: string,
  registry: BackendEntry[],
  memo: ReachabilityMemo,
  nowMs: number,
): RoutePlan {
  if (selection !== AUTO_SELECTION) {
    const pinned = findBackendEntry(registry, selection);
    if (pinned !== null) {
      if (isFreshDead(memo, pinned.id, nowMs)) {
        return { kind: "declined", entry: pinned, fallback: fallbackEntry(registry, pinned.id) };
      }
      return { kind: "sequence", steps: [{ kind: "attempt", entry: pinned }] };
    }
    // Falls through to Auto below.
  }

  return {
    kind: "sequence",
    steps: registry.map((entry) =>
      isFreshDead(memo, entry.id, nowMs) ? { kind: "skip", entry } : { kind: "attempt", entry },
    ),
  };
}
