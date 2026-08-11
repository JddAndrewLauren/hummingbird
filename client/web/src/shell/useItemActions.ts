import type { TaskActionName } from "../store/protocol";
import { actOnTask, type WorkerLike } from "../store/worker-client";

// S11/#109's act wiring: item detail's start/complete/block/cancel buttons
// call this rather than posting to `worker` directly, the same one-line
// wrapper shape `App.tsx`'s other shell hooks already use for the task
// binding's send helpers.
//
// `Core::act`'s own overlay update is synchronous and offline-safe — this
// hook does not wait for `actResult` before returning, and does not need
// to: `worker-client.ts`'s `actResult` handler re-requests the frontier and
// blocked queries itself the moment a successful result broadcasts, which
// is what makes the mutation visible immediately (this issue's "Completing
// offline shows Done immediately").
export interface ItemActions {
  act: (itemId: string, action: TaskActionName) => void;
}

/** Mints this act's seed. Deterministic — `client/core/src/sync/mod.rs`'s
 * seed-minting rule (#223): acting touches an item that already exists, so
 * the seed's hash becomes only the mutation's local queue-entry id, and
 * retrying the identical intent (same item, same action, same `nowMs`)
 * must reproduce the identical entry rather than enqueue a second one.
 * Exported standalone, not inlined in the hook body, so it is directly
 * testable without rendering a component — `useCaptureWiring.ts`'s own
 * `mintSeed` split. */
export function mintActSeed(itemId: string, action: TaskActionName, nowMs: number): string {
  return `${itemId}:${action}:${nowMs}`;
}

export function useItemActions(worker: WorkerLike): ItemActions {
  function act(itemId: string, action: TaskActionName): void {
    const nowMs = Date.now();
    const seed = mintActSeed(itemId, action, nowMs);
    actOnTask(worker, seed, itemId, action, nowMs);
  }

  return { act };
}
