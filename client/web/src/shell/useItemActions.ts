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

export function useItemActions(worker: WorkerLike): ItemActions {
  function act(itemId: string, action: TaskActionName): void {
    const nowMs = Date.now();
    // Deterministic, not random: same "caller-injected, no clock/RNG that
    // panics on bare wasm32" reasoning `Core::act`'s own `seed` parameter
    // documents. Item + action + millisecond timestamp is unique enough for
    // one person clicking one button at a time.
    const seed = `${itemId}:${action}:${nowMs}`;
    actOnTask(worker, seed, itemId, action, nowMs);
  }

  return { act };
}
