import { useEffect, useState } from "react";
import { requestSteps, type WorkerLike } from "../store/worker-client";

// Item detail's own small piece of shell wiring (issue #96, S10): which
// item (if any) is open, and asking the worker for its Steps the moment
// selection changes. Description needs no request of its own — it already
// lives on the `TaskItemDTO` the frontier/blocked queries returned.
export interface ItemDetailWiring {
  selectedItemId: string | null;
  openItem: (itemId: string) => void;
  closeItem: () => void;
}

export function useItemDetailWiring(worker: WorkerLike): ItemDetailWiring {
  const [selectedItemId, setSelectedItemId] = useState<string | null>(null);

  useEffect(() => {
    if (selectedItemId !== null) {
      requestSteps(worker, selectedItemId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedItemId]);

  return {
    selectedItemId,
    openItem: setSelectedItemId,
    closeItem: () => setSelectedItemId(null),
  };
}
