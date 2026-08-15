// S11/#109's act affordances: which buttons item detail offers for an
// item's current stage. A pure function — item detail (`ItemDetailPanel`)
// calls this rather than branching on `item.stage` inline, so the mapping
// itself is unit-testable without React.
//
// Triage and Grilling offer nothing in `availableActions` — neither is an
// action yet (`CONTEXT.md`'s "Stage" glossary entry — "Triage and Grilling
// are pre-action by definition"), and promoting one is S13's triage screen,
// not this vocabulary. Done offers nothing either — a finished item has
// nothing left to act on. There is no "pick" affordance because there is no
// state for it: the frontier (S10, `Core::frontier`) already IS "what can be
// started right now" — `"start"` on a Ready item is the only promotion this
// slice makes, and it lands on `InProgress`, never a distinct "picked" stage.
//
// **Triage now offers one gesture of its own: Grill me** (#355, ADR-0023) —
// `canGrill` below, deliberately NOT folded into `availableActions`'s
// `TaskActionName` vocabulary. A Grill is not an `ItemAction`: it opens the
// interview takeover rather than mutating the item directly, and its
// eventual stage move comes from `hummingbird-domain`'s own verdict
// function (`resulting_stage`), never from this file's mapping. Grilling
// itself offers no such button — an item already mid-interview has nothing
// further to grill until this one's transcript resolves — so `canGrill` is
// `true` for `"triage"` alone, the same "one deciding function" discipline
// `canMarkDone` documents for its own, wider vocabulary.
import type { TaskActionName, TaskItemDTO, TaskStageName } from "../store/protocol";

const ACTIONS_BY_STAGE: Record<TaskStageName, readonly TaskActionName[]> = {
  triage: [],
  grilling: [],
  // `"complete"` from Ready (and Blocked below) is the amendment the row
  // checkmark made: finishing is one click from any live stage, because
  // "I did it" is a fact about the world, not about whether the app was
  // told the item had been started first. `Core::act` never gated on the
  // current stage anyway — this vocabulary was the only gate.
  ready: ["start", "complete", "block", "cancel"],
  // Resuming a stalled `in_progress` item back into `in_progress` is a
  // no-op the UI never offers; only forward (`complete`) or sideways
  // (`block`, `cancel`) actions apply.
  in_progress: ["complete", "block", "cancel"],
  // `Blocked` means an external wait ended (`CONTEXT.md`) — the way
  // back onto the frontier is `"start"`, exactly as if the item were
  // freshly Ready, and `"complete"` covers the wait that ended because
  // the item turned out finished. `"block"` is deliberately absent: an
  // already-blocked item offers no "block it again" affordance.
  blocked: ["start", "complete", "cancel"],
  done: [],
};

export function availableActions(stage: TaskStageName): readonly TaskActionName[] {
  return ACTIONS_BY_STAGE[stage] ?? [];
}

/** Whether a row offers the one-click "mark done" checkmark: any live,
 * unarchived stage but Done itself. Deliberately WIDER than
 * `availableActions` — Triage and Grilling stay pre-action in the detail
 * panel's vocabulary (no start/block there), but a capture that turned out
 * already finished is still one click, which is the amendment the checkmark
 * decision made to "pre-action by definition". The one deciding function for
 * every screen's row, so the affordance cannot drift between them. */
export function canMarkDone(item: Pick<TaskItemDTO, "stage" | "archivedAt">): boolean {
  return item.stage !== "done" && item.archivedAt === null;
}

/** Whether a row offers "Grill me" (#355, ADR-0023): Triage rows only, this
 * slice — the tracer deliberately does not extend the affordance to any
 * other stage. Since #360, promoting an item straight into Grilling is no
 * longer a triage gesture at all: this is the only way an item gets there. */
export function canGrill(stage: TaskStageName): boolean {
  return stage === "triage";
}

/** The Grill button's own label (#356, ADR-0023): "Resume grill" when this
 * item already carries a draft, "Grill me" otherwise — decided by this ONE
 * function rather than a per-screen branch on `hasDraft`, the same "one
 * deciding function" discipline `canGrill`/`canMarkDone` document for their
 * own affordance. */
export function grillButtonLabel(hasDraft: boolean): "Grill me" | "Resume grill" {
  return hasDraft ? "Resume grill" : "Grill me";
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
/** The round-2 PR #207 fix: what `pending` the detail panel's FALLBACK item
 * (an item that has left both `frontier` and `blocked` — a just-blocked or
 * just-cancelled one) should render, and whether the screen is still waiting
 * for the act's own `isPending` read to land.
 *
 * Why: `applyItemAction`'s `pending: true` is a frozen snapshot. A
 * `Stage::Blocked` item never re-enters either live query, so nothing ever
 * replaced that snapshot and the Start/Cancel row rendered permanently
 * disabled (`ItemDetailPanel`'s `disabled={item.pending}`) — functionally
 * unreachable. The live source is `TaskState.pending[id]`, fed by
 * `worker-client.ts`'s `isPending` re-read on every ok `actResult` and by
 * `useItemDetailWiring`'s per-sync-cycle re-read for the open item; once the
 * queued mutation drains, that read flips to `false` and the row enables.
 *
 * `awaitingConfirm` covers the one stale window: clicking a SECOND act (e.g.
 * Start on the now-enabled blocked row) fires while `TaskState.pending[id]`
 * still holds the previous act's drained `false`. The screen sets
 * `awaitingConfirm` at click time, and this function refuses to let a live
 * `false` enable the row until it has observed the new act's own `true`
 * once — so the row can never briefly re-enable mid-mutation.
 *
 * Pure on purpose (reviewer note on PR #207): the repo has no component-test
 * infrastructure, so the deciding logic lives here where a vitest node test
 * can execute it; `NowScreen.tsx` only threads React state through it. */
export interface FallbackPendingResolution {
  /** What the rendered item's `pending` should be this render. */
  pending: boolean;
  /** The next `awaitingConfirm` state the screen should hold. */
  awaitingConfirm: boolean;
}

export function resolveFallbackPending(
  optimisticPending: boolean,
  livePending: boolean | undefined,
  awaitingConfirm: boolean,
): FallbackPendingResolution {
  if (awaitingConfirm) {
    // Disabled until the fresh act is confirmed queued; a live `true`
    // is that confirmation, after which the live value takes over.
    return { pending: true, awaitingConfirm: livePending !== true };
  }
  return { pending: livePending ?? optimisticPending, awaitingConfirm: false };
}

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
