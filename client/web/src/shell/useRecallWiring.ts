import { useEffect } from "react";
import { coreStore, type CoreStatus, type TaskState } from "../store/store";
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
//
// **Why two effects, and why the clear keys on `query` ALONE.**
// `TaskState.search` is one unkeyed slot and no query echo travels back on
// the `searchResult` message, so between a keystroke and its answer the
// previous query's rows are still what `App.tsx` renders — rows a reader can
// now expand and EDIT (#479), under a query that no longer selects them.
// Clearing the slot the instant the query changes is what makes that
// impossible: `RecallOverlay` renders `rows === null` under a non-empty
// query as "Searching…", so a cleared slot and a never-set one are the same
// state to it. The clear must NOT key on the request effect's other deps: a
// `lastTriage`-driven re-ask is the refresh of a row that is still on screen
// and being edited, and clearing there would flash "Searching…" over the
// open panel — undoing the very fix `lastTriage` exists for. It writes the
// store singleton from inside the hook, `useTaskTokenWiring.ts`'s precedent:
// the fact belongs to this wiring, not to `App.tsx`.
export function useRecallWiring(
  worker: WorkerLike,
  status: CoreStatus,
  query: string,
  lastTriage: TaskState["lastTriage"],
): void {
  const ready = status === "ready";
  const trimmed = query.trim();

  // The `!== null` guard keeps the app's first render — and every render
  // under an already-empty slot — from a pointless store notify.
  useEffect(() => {
    if (coreStore.getSnapshot().task.search !== null) {
      coreStore.setTaskState({ search: null });
    }
  }, [query]);

  useEffect(() => {
    if (!ready || trimmed.length === 0) {
      return;
    }
    requestSearch(worker, query, Date.now());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, trimmed, query, lastTriage]);
}
