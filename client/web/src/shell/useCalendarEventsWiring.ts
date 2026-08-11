import { useEffect } from "react";
import type { CoreStatus } from "../store/store";
import { requestCalendarEvents, type WorkerLike } from "../store/worker-client";

// Issue #267's calendar-events wiring: the generic request-on-signal glue
// for `getCalendarEvents`, on the exact "once ready, then per completed
// cycle" shape `usePaneReadsWiring.ts` uses for `getPaneRead` — and for the
// same reason. A completed sync cycle is exactly when a fresh calendar poll
// (ADR-0005's own foreground timer, driven by `useCalendarWiring.ts`) can
// have landed a new snapshot.
//
// **Owns no policy about WHICH intervals to ask for.** Unlike
// `usePaneReadsWiring.ts`, which reads `registry.ts`'s `requiredSources()`
// itself, this hook takes its `requests` as a prop: #267 builds the seam,
// not a standing question, and no question is registered against the
// calendar arm yet (that is #122's job — the Agent Brief for this issue is
// explicit that registering one is out of scope here). A caller with
// nothing to ask for passes an empty array, and the hook — correctly —
// requests nothing.
//
// Thin glue and **no clock of its own**: `Date.now()` below is the
// request's own `nowMs`, the instant `freshness` is measured against,
// core-side — not a timer. No `setInterval`/`setTimeout` is added here or
// anywhere this hook touches: ADR-0007's single 60-second SharedWorker
// interval is still the only clock the origin gets.

export interface CalendarEventsRequest {
  /** The caller's own identity for this request — becomes the
   * `QuestionInputs.calendarReads` map key, so a request from a caller that
   * reuses the same `key` across renders lands in the same slot rather than
   * growing the map without bound. */
  key: string;
  startMs: number;
  endMs: number;
}

export function useCalendarEventsWiring(
  worker: WorkerLike,
  status: CoreStatus,
  /** `TaskState.syncOutcomeSeq` — see `usePaneReadsWiring.ts`'s own
   * parameter doc for why this, not the outcome's `kind`, is what a
   * per-cycle refresh must key on. */
  syncOutcomeSeq: number,
  requests: readonly CalendarEventsRequest[],
): void {
  const ready = status === "ready";
  // Requests are compared by value, not by the array's own identity: a
  // caller that recomputes an equivalent array on every render (the common
  // case for a derived "this weekend's interval") must not re-fire the
  // effect every render just because the reference changed underneath an
  // unchanged value.
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
