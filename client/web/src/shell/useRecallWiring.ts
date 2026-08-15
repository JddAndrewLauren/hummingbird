import { useEffect } from "react";
import type { CoreStatus } from "../store/store";
import { requestSearch, type WorkerLike } from "../store/worker-client";

// **Recall**'s read-side wiring (#478, CONTEXT.md): a thin effect requesting
// `Core::search` whenever the query changes, once the core is ready — the
// same shape `useLedgerWiring.ts` keeps, but keyed on the query itself
// rather than on a sync cycle or a mutation result, since nothing about a
// Recall query is a mutation for anything else to invalidate.
//
// An empty or whitespace-only query is never sent: `RecallOverlay` already
// renders its own "type to search" state for one without needing an
// answer, and sending it anyway would only ask the core to restate a rule
// this side already knows (decision: "an empty query lists nothing").
//
// **No debounce.** Recall's matcher is a read-time scan over the mirror
// (ADR-0002, same as every other core read here), not a network request —
// there is no round trip to spare a request against, so every keystroke
// simply asks again.
export function useRecallWiring(worker: WorkerLike, status: CoreStatus, query: string): void {
  const ready = status === "ready";
  const trimmed = query.trim();

  useEffect(() => {
    if (!ready || trimmed.length === 0) {
      return;
    }
    requestSearch(worker, query, Date.now());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, trimmed, query]);
}
