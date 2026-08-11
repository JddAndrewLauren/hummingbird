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
// Thin glue and **no clock of its own**: `Date.now()` is the request's own
// `nowMs`, the instant both "which interval" (a declared window like #122's
// rolling weekend one can itself be a function of "now") and `freshness`
// are measured against, core-side — not a timer. No `setInterval`/
// `setTimeout` is added here or anywhere this hook touches: ADR-0007's
// single 60-second SharedWorker interval is still the only clock the
// origin gets.
//
// `Date.now()` is read **inside** the effect, never during render
// (react-hooks/purity — a render is allowed to run more than once, or be
// thrown away, for reasons that have nothing to do with this hook, and a
// clock read there would silently let a re-render decide which interval
// gets requested). Because `requiredCalendarRequests()` is now called only
// once per actual effect run, there is no longer a render-computed array
// whose identity needs guarding against — the old `JSON.stringify`
// by-value key existed solely to keep that render-time array out of the
// effect's own dependency list without disabling `exhaustive-deps`, and
// that problem disappears once nothing but the effect ever calls it.

export function useCalendarEventsWiring(
  worker: WorkerLike,
  status: CoreStatus,
  /** `TaskState.syncOutcomeSeq` — see `usePaneReadsWiring.ts`'s own
   * parameter doc for why this, not the outcome's `kind`, is what a
   * per-cycle refresh must key on. */
  syncOutcomeSeq: number,
): void {
  const ready = status === "ready";

  useEffect(() => {
    if (!ready) {
      return;
    }
    const nowMs = Date.now();
    for (const request of requiredCalendarRequests(nowMs)) {
      requestCalendarEvents(
        worker,
        request.key,
        request.startMs,
        request.endMs,
        request.startDate,
        request.endDate,
        nowMs,
      );
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, syncOutcomeSeq]);
}
