import { useEffect } from "react";
import { requiredCalendarRequests } from "../screens/questions/registry";
import type { CoreStatus } from "../store/store";
import { requestCalendarEvents, type WorkerLike } from "../store/worker-client";

// Issue #267's calendar-events wiring: the generic request-on-signal glue
// for `getCalendarEvents`, on the exact "once ready, then per completed
// cycle" shape `usePaneReadsWiring.ts` uses for `getPaneRead` — and for the
// same reason. A completed sync cycle is exactly when a fresh calendar poll
// (ADR-0005's own foreground timer, driven by `useCalendarWiring.ts`) can
// have landed a new snapshot.
//
// Which intervals to ask for is `registry.ts`'s `requiredCalendarRequests()`
// answer, exactly the way `usePaneReadsWiring.ts` reads `requiredSources()`
// itself rather than taking a `requests` prop: a caller-supplied list would
// re-open the drift `requiredSources()` closes for the snapshot arm — #122
// could register a calendar-lane question and have it silently never
// requested, because nothing forces this hook's caller to also update. #122
// registered the weekend-plans pane as the first (and, today, only)
// calendar-lane question, so `requiredCalendarRequests()` now returns its
// one rolling window rather than the empty list this hook used to be
// mounted against.
//
// Thin glue and **no clock of its own**: `Date.now()` below is the
// request's own `nowMs`, the instant `freshness` is measured against,
// core-side — not a timer. No `setInterval`/`setTimeout` is added here or
// anywhere this hook touches: ADR-0007's single 60-second SharedWorker
// interval is still the only clock the origin gets.

export function useCalendarEventsWiring(
  worker: WorkerLike,
  status: CoreStatus,
  /** `TaskState.syncOutcomeSeq` — see `usePaneReadsWiring.ts`'s own
   * parameter doc for why this, not the outcome's `kind`, is what a
   * per-cycle refresh must key on. */
  syncOutcomeSeq: number,
): void {
  const ready = status === "ready";
  // The registry's own clock reasoning (`registry.ts`'s `requiredCalendarRequests`
  // doc): a declared interval — #122's rolling weekend window — can itself
  // be a function of "now", so this render's own instant decides which
  // interval is asked for. The effect below still reads its own fresh
  // `Date.now()` for the request's `nowMs` (the freshness clock), exactly
  // as before; this one only decides WHICH interval.
  const requests = requiredCalendarRequests(Date.now());
  // Requests are compared by value, not by the array's own identity: the
  // registry recomputing an equivalent array on every call must not re-fire
  // the effect every render just because the reference changed underneath
  // an unchanged value.
  const requestsKey = JSON.stringify(requests);

  useEffect(() => {
    if (!ready) {
      return;
    }
    const nowMs = Date.now();
    for (const request of requests) {
      requestCalendarEvents(worker, request.key, request.startMs, request.endMs, nowMs);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, syncOutcomeSeq, requestsKey]);
}
