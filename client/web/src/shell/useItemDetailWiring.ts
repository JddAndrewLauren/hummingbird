import { useEffect, useState } from "react";
import { requestIsPending, requestSteps, type WorkerLike } from "../store/worker-client";

// Item detail's own small piece of shell wiring (issue #96, S10): which
// item (if any) is open, and asking the worker for its Steps the moment
// selection changes. Description needs no request of its own — it already
// lives on the `TaskItemDTO` the frontier/blocked queries returned.
export interface ItemDetailWiring {
  selectedItemId: string | null;
  openItem: (itemId: string) => void;
  closeItem: () => void;
}

export function useItemDetailWiring(worker: WorkerLike, syncOutcomeSeq: number): ItemDetailWiring {
  const [selectedItemId, setSelectedItemId] = useState<string | null>(null);

  useEffect(() => {
    if (selectedItemId !== null) {
      requestSteps(worker, selectedItemId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedItemId]);

  // PR #207 round-2 fix: the open item's `pending` renders from
  // `TaskState.pending`, so it must be re-read whenever a sync cycle could
  // have drained the queued mutation — keyed on `syncOutcomeSeq` exactly
  // like `useFrontierWiring`'s per-cycle refresh (no timer of its own;
  // ADR-0007's single interval stays the only clock). Without this the
  // `false` that re-enables a blocked item's Start/Cancel row never arrives.
  useEffect(() => {
    if (selectedItemId !== null) {
      requestIsPending(worker, selectedItemId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedItemId, syncOutcomeSeq]);

  return {
    selectedItemId,
    openItem: setSelectedItemId,
    closeItem: () => setSelectedItemId(null),
  };
}
