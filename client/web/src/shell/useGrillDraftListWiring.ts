import { useEffect } from "react";
import type { CoreStatus } from "../store/store";
import { requestGrillDraftItemIds, type WorkerLike } from "../store/worker-client";

// #356/ADR-0023's bulk draft-list read: every item id carrying a Grill
// draft, requested once the core is ready — the Triage inbox's "Resume
// grill" labels. Requested exactly once per session, unlike
// `useBindingsWiring.ts`'s own per-sync-cycle refresh: a draft never syncs
// (it never rides a delta or a sweep), so nothing about a completed cycle
// could ever change this list. The only things that DO change it — this
// tab's own save/discard, or another tab's, both broadcast to every
// connected port (ADR-0010) — already trigger their own refresh in
// `worker/task-worker.ts`'s `saveGrillDraft`/`discardGrillDraft` handling,
// so there is nothing else for this hook to key a refresh on.
export function useGrillDraftListWiring(worker: WorkerLike, status: CoreStatus): void {
  const ready = status === "ready";
  useEffect(() => {
    if (!ready) return;
    requestGrillDraftItemIds(worker);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready]);
}
