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
import type { TaskActionName, TaskStageName } from "../store/protocol";

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
