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
// Which sources to ask for is the core's answer (#820), reached through
// `registry.ts`'s `requiredSources`: `panes::question_sources` is the one
// declaration of which question reads which source, and the mobile seam's
// pane-read loop reads the same union, so a source added there is requested
// here without this file — or any per-question TS list — changing.
//
// This hook is not screen-scoped — it fires once, regardless of which
// screen is showing — so it asks `requiredSources` for BOTH surfaces
// (ADR-0017) and unions them: switching from Now to Status must never wait
// on a fetch this hook could already have made.

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
    const sources = new Set([...requiredSources("now"), ...requiredSources("status")]);
    for (const source of sources) {
      requestPaneRead(worker, source, nowMs);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, syncOutcomeSeq]);
}
