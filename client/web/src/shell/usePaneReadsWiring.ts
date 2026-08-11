import { useEffect } from "react";
import { requiredSources } from "../screens/questions/registry";
import type { CoreStatus } from "../store/store";
import { requestPaneRead, type WorkerLike } from "../store/worker-client";

// #245's pane-read wiring: asks for every source the registered standing
// questions need, once the core is ready and again after every sync cycle —
// the same "refresh once ready, then per-cycle" shape `useBindingsWiring.ts`
// uses, and for the same reason. A completed cycle is exactly when a new
// `context_snapshots` row or a new alert can have arrived.
//
// Thin glue and **no clock of its own**: ADR-0007's single 60-second interval
// lives in the SharedWorker, and nothing here schedules anything. The
// `Date.now()` below is the request's own `nowMs` — the instant the ages and
// the alert-liveness filter are resolved against, core-side — not a timer.
//
// Which sources to ask for is `registry.ts`'s answer, unioned over every
// registered question: a question added to the registry is requested here
// without this file changing, which is what stops the two lists drifting.

export function usePaneReadsWiring(
  worker: WorkerLike,
  status: CoreStatus,
  /** `TaskState.syncOutcomeSeq` — see `useFrontierWiring.ts`'s own parameter
   * doc for why this, not the outcome's `kind`, is what a per-cycle refresh
   * keys on. */
  syncOutcomeSeq: number,
): void {
  const ready = status === "ready";

  useEffect(() => {
    if (!ready) {
      return;
    }
    const nowMs = Date.now();
    for (const source of requiredSources()) {
      requestPaneRead(worker, source, nowMs);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, syncOutcomeSeq]);
}
