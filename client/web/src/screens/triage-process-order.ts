// #357: the "triage process" queue (CONTEXT.md — the pair of pre-action
// stages, Triage and Grilling, together) as ONE combined, ordered read.
// Before this, "what is in the triage process" was inferred independently by
// the Triage screen, Now's collapsible triage area and whatever renders the
// counts — three call sites for one fact, which is how the two surfaces
// drift. This is the one function both read; neither screen may filter by
// stage on its own.
//
// Order: local drafts first, then Grilling-stage items, then captured
// Triage items. A draft is device-local (#356) and can sit on an item of
// either pre-action stage — it wins the front of the queue regardless of
// which stage it came from, since resuming an already-started interview is
// the most actionable thing in the pile. Within each of the three groups,
// order is `orderTriage`'s own oldest-capture-first (unchanged for the
// captured group, and reused for the other two for the same reason: one
// deterministic tiebreak, not three).

import type { TaskItemDTO } from "../store/protocol";
import { orderTriage } from "./triage-order";

export interface TriageProcessQueue {
  /** The combined, ordered read both surfaces render. */
  items: TaskItemDTO[];
  /** Exact count of live Triage-stage items — "captured" in the header,
   * whatever their draft status. Never folded into `grillingCount`: the
   * whole point of this shape is that the two are never a single
   * undifferentiated total. */
  capturedCount: number;
  /** Exact count of live Grilling-stage items. */
  grillingCount: number;
}

/** Combines `triageItems` (`TaskState.triageInbox`, Stage::Triage only) and
 * `grillingItems` (`TaskState.grillingItems`, Stage::Grilling only) into one
 * ordered queue, with `draftItemIds` (`TaskState.grillDraftItemIds`) deciding
 * which items are drafts. Pure: never mutates either input array, and
 * reading it twice with the same input yields the same output. */
export function triageProcessQueue(
  triageItems: readonly TaskItemDTO[],
  grillingItems: readonly TaskItemDTO[],
  draftItemIds: readonly string[],
): TriageProcessQueue {
  const draftIds = new Set(draftItemIds);
  const isDraft = (item: TaskItemDTO) => draftIds.has(item.id);

  const drafts = orderTriage([...triageItems, ...grillingItems].filter(isDraft));
  const grilling = orderTriage(grillingItems.filter((item) => !isDraft(item)));
  const captured = orderTriage(triageItems.filter((item) => !isDraft(item)));

  return {
    items: [...drafts, ...grilling, ...captured],
    capturedCount: triageItems.length,
    grillingCount: grillingItems.length,
  };
}
