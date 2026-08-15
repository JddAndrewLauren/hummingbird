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
  /** `destination` is `null` (#122) for a pure field edit that leaves
   * `stage` untouched — see `worker-client.ts`'s `triageItem` doc. */
  triage: (itemId: string, destination: "ready" | null, edits: TriageEdits) => void;
}

/** Mints this triage's seed. Deterministic — `client/core/src/sync/mod.rs`'s
 * seed-minting rule (#223): triaging touches an item that already exists,
 * so the seed's hash becomes only the mutation's local queue-entry id, and
 * retrying the identical intent (same item, same destination, same `nowMs`)
 * must reproduce the identical entry rather than enqueue a second one.
 * Exported standalone, not inlined in the hook body, so it is directly
 * testable without rendering a component — `useCaptureWiring.ts`'s own
 * `mintSeed` split. */
export function mintTriageSeed(
  itemId: string,
  destination: "ready" | null,
  nowMs: number,
): string {
  return `${itemId}:triage:${destination ?? "none"}:${nowMs}`;
}

export function useTriageWiring(worker: WorkerLike): TriageWiring {
  function triage(itemId: string, destination: "ready" | null, edits: TriageEdits): void {
    const nowMs = Date.now();
    const seed = mintTriageSeed(itemId, destination, nowMs);
    triageItem(worker, seed, itemId, destination, edits, nowMs);
  }

  return { triage };
}
