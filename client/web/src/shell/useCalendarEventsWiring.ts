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
// requested, because nothing forces this hook's caller to also update. It
// returns empty today, since no shipped question reads the calendar arm yet
// (that is #122's job); mounted here with today's empty list, this hook
// still has a real caller, which is the point.
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
  const requests = requiredCalendarRequests();
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
