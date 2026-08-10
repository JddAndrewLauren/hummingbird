// Triage's display order (issue #110/S12's "three captures then
// reconnecting produces three Triage items in order" acceptance criterion).
// Unlike the frontier (`frontier-order.ts`'s priority/deadline ranking,
// which a freshly captured item has no opinion on — priority defaults to 0
// and there is no deadline at capture, ADR-0009), Triage has exactly one
// natural order: the sequence things were captured in. `createdAt` is what
// both a still-pending overlay (stamped with the capture's own `nowMs`,
// `client/core/src/lib.rs`'s `item_from_create`) and a server-confirmed row
// carry, so this reads the same before and after a sync cycle lands.

import type { TaskItemDTO } from "../store/protocol";

/** A pure sort: never mutates `items`, and reading it twice with the same
 * input yields the same output — no clock, no randomness, nothing ambient.
 * Oldest capture first; `id` is the tie-break for two items that landed in
 * the same millisecond. */
export function orderTriage(items: readonly TaskItemDTO[]): TaskItemDTO[] {
  return [...items].sort((a, b) => {
    if (a.createdAt !== b.createdAt) {
      return a.createdAt - b.createdAt;
    }
    return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
  });
}
