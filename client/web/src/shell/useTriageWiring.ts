import type { TriageDestinationName } from "../store/protocol";
import { triageItem, type TriageEdits, type WorkerLike } from "../store/worker-client";

// S13/#111's triage wiring: `TriageScreen`'s per-item promote buttons call
// this rather than posting to `worker` directly — the same one-line wrapper
// shape `useItemActions.ts` already uses for S11's act buttons.
//
// `Core::triage`'s own overlay update is synchronous and offline-safe (this
// issue's "triaging offline queues correctly and reconciles on the next
// cycle") — this hook does not wait for `triageResult` before returning;
// `worker-client.ts`'s `triageResult` handler re-requests the triage inbox
// and frontier itself the moment a successful result broadcasts.
export interface TriageWiring {
  triage: (itemId: string, destination: TriageDestinationName, edits: TriageEdits) => void;
}

export function useTriageWiring(worker: WorkerLike): TriageWiring {
  function triage(itemId: string, destination: TriageDestinationName, edits: TriageEdits): void {
    const nowMs = Date.now();
    // Deterministic, not random — same "caller-injected, no clock/RNG that
    // panics on bare wasm32" reasoning `Core::triage`'s own `seed` parameter
    // documents (mirrors `useItemActions.ts`'s own seed shape).
    const seed = `${itemId}:triage:${destination}:${nowMs}`;
    triageItem(worker, seed, itemId, destination, edits, nowMs);
  }

  return { triage };
}
