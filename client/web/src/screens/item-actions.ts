// S11/#109's act affordances: which buttons item detail offers for an
// item's current stage. A pure function — item detail (`ItemDetailPanel`)
// calls this rather than branching on `item.stage` inline, so the mapping
// itself is unit-testable without React.
//
// Triage and Grilling offer nothing here: neither is an action yet
// (`CONTEXT.md`'s "Stage" glossary entry — "Triage and Grilling are
// pre-action by definition"), and promoting one is S13's triage screen, not
// this slice. Done offers nothing either — a finished item has nothing left
// to act on. There is no "pick" affordance because there is no state for
// it: the frontier (S10, `Core::frontier`) already IS "what can be started
// right now" — `"start"` on a Ready item is the only promotion this slice
// makes, and it lands on `InProgress`, never a distinct "picked" stage.
import type { TaskActionName, TaskItemDTO, TaskStageName } from "../store/protocol";

const ACTIONS_BY_STAGE: Record<TaskStageName, readonly TaskActionName[]> = {
  triage: [],
  grilling: [],
  ready: ["start", "block", "cancel"],
  // Resuming a stalled `in_progress` item back into `in_progress` is a
  // no-op the UI never offers; only forward (`complete`) or sideways
  // (`block`, `cancel`) actions apply.
  in_progress: ["complete", "block", "cancel"],
  // `Blocked` means an external wait ended (`CONTEXT.md`) — the only way
  // back onto the frontier is `"start"`, exactly as if the item were
  // freshly Ready. `"block"` is deliberately absent: an already-blocked
  // item offers no "block it again" affordance.
  blocked: ["start", "cancel"],
  done: [],
};

export function availableActions(stage: TaskStageName): readonly TaskActionName[] {
  return ACTIONS_BY_STAGE[stage] ?? [];
}

/** Mirrors `hummingbird_core::ItemAction::stage`'s mapping — the same
 * closed action-to-stage vocabulary, restated here only for the UI's own
 * optimistic display (`NowScreen.tsx`'s `RealFrontier`).
 *
 * Why this exists at all: `"block"` and `"cancel"` both move an item out of
 * every query the store can currently re-fetch (`getFrontier`/`getBlocked`
 * — `Stage::Blocked` and an archived item are outside both, S10's own
 * scope), so waiting for the next server round trip to show the result
 * would mean the item detail panel either goes stale (still showing the
 * pre-action stage) or has nothing live to render at all. This produces the
 * same post-mutation `TaskItemDTO` `Core::act`'s own overlay already
 * computed, so the panel can display it immediately without inventing a
 * second source of truth — it is a read-time projection, never sent
 * anywhere, and the next successful sync cycle's `frontier`/`blocked`
 * refresh (or dead-letter revert) is what makes the real value authoritative
 * again. */
export function applyItemAction(item: TaskItemDTO, action: TaskActionName): TaskItemDTO {
  switch (action) {
    case "start":
      return { ...item, stage: "in_progress", pending: true };
    case "complete":
      return { ...item, stage: "done", pending: true };
    case "block":
      return { ...item, stage: "blocked", pending: true };
    case "cancel":
      // `hummingbird_domain::Item::archived_at` is `ms epoch | null` — any
      // non-null value marks it archived; the exact timestamp is never
      // read back from this projection (`Core::act`'s own enqueue call
      // carries the real one).
      return { ...item, archivedAt: Date.now(), pending: true };
  }
}
