import { useEffect } from "react";
import type { CoreStatus, TaskState } from "../store/store";
import { requestDone, requestLedger, type WorkerLike } from "../store/worker-client";

// The Ledger and Done screens' read-side wiring: requests both once the core
// is ready, again after every sync cycle (the same "refresh once ready, then
// per-cycle" shape `useFrontierWiring.ts` uses), and — unlike that hook —
// also on every mutation *result*. `worker-client.ts` re-reads the frontier
// behind an ok `actResult` itself, but it deliberately takes no clock and
// `getLedger` carries a `nowMs` (the alert badge's liveness instant), so the
// immediate re-read lives here instead, keyed on the result slots: a
// capture, act or triage taken offline shows on these screens without
// waiting for a cycle, exactly as it does on Now.
//
// Thin glue and **no clock of its own**: the `Date.now()` below is the
// request's own `nowMs`, not a timer — `usePaneReadsWiring.ts` documents the
// same distinction.
export function useLedgerWiring(
  worker: WorkerLike,
  status: CoreStatus,
  /** `TaskState.syncOutcomeSeq` — see `useSyncWiring.ts`'s own parameter
   * doc for why this, not the outcome's `kind`, is what a per-cycle refresh
   * must key on. */
  syncOutcomeSeq: number,
  /** The three mutation-result slots (`TaskState.lastCapture`/`lastAct`/
   * `lastTriage`): each is a fresh object per resolved mutation, so object
   * identity in the dependency list is exactly "a mutation just resolved". */
  lastCapture: TaskState["lastCapture"],
  lastAct: TaskState["lastAct"],
  lastTriage: TaskState["lastTriage"],
): void {
  const ready = status === "ready";

  useEffect(() => {
    if (!ready) {
      return;
    }
    requestLedger(worker, Date.now());
    requestDone(worker);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, syncOutcomeSeq, lastCapture, lastAct, lastTriage]);
}
