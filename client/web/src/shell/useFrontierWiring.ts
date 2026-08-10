import { useEffect } from "react";
import type { CoreStatus } from "../store/store";
import { requestBlocked, requestFrontier, requestProjects, type WorkerLike } from "../store/worker-client";

// S10's read-side wiring (issue #108): requests the frontier and the
// relation-blocked explanation once the core is ready, and again after
// every sync cycle — the same "refresh once ready, then per-cycle" shape
// `useSyncWiring.ts` already uses for the sync-status reads, since a
// completed cycle is exactly when either query's answer can have changed.
// This hook is deliberately thin glue: the actual ordering/grouping/urgency
// decisions are unit-tested pure modules (`frontier-order.ts`,
// `frontier-groups.ts`, `urgency.ts`) the screen calls directly, not
// anything here.
export function useFrontierWiring(
  worker: WorkerLike,
  status: CoreStatus,
  /** `TaskState.syncOutcomeSeq` — see `useSyncWiring.ts`'s own parameter
   * doc for why this, not the outcome's `kind`, is what a per-cycle refresh
   * must key on. */
  syncOutcomeSeq: number,
): void {
  const ready = status === "ready";

  useEffect(() => {
    if (!ready) {
      return;
    }
    requestFrontier(worker);
    requestBlocked(worker);
    requestProjects(worker);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, syncOutcomeSeq]);
}
