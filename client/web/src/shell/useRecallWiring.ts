import { useEffect } from "react";
import type { CoreStatus, TaskState } from "../store/store";
import { requestSearch, type WorkerLike } from "../store/worker-client";

// **Recall**'s read-side wiring (#478, CONTEXT.md): a thin effect requesting
// `Core::search` whenever the query changes, once the core is ready — the
// same shape `useLedgerWiring.ts` keeps.
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
//
// **`lastTriage` (#479).** Once a result row's expansion could edit a live
// item (`RecallOverlay`'s `ItemPanel`), a Recall query *does* have a
// mutation to invalidate: an edit lands in the mirror, but `rows` is only
// ever the answer to the query as it stood at the moment it was last
// asked, so without this the row (and the panel built from it) would keep
// showing the pre-edit title until some unrelated re-render happened to
// re-run this effect. Keyed on `lastTriage` object identity, the same
// "a fresh mutation-result object is exactly one resolved write" contract
// `useLedgerWiring.ts` already relies on — re-asking the identical query the
// instant a triage resolves is what keeps a just-edited row in step with
// what was just typed into it.
export function useRecallWiring(
  worker: WorkerLike,
  status: CoreStatus,
  query: string,
  lastTriage: TaskState["lastTriage"],
): void {
  const ready = status === "ready";
  const trimmed = query.trim();

  useEffect(() => {
    if (!ready || trimmed.length === 0) {
      return;
    }
    requestSearch(worker, query, Date.now());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, trimmed, query, lastTriage]);
}
