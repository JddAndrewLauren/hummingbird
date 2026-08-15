// S11/#109's act affordances, plus #355/#359/ADR-0023's Grill affordance:
// which buttons item detail offers for an item's current stage, and which
// stage an act vocabulary word sets.
//
// The rules no longer live here. They are `hummingbird_core::decisions::actions`
// (ADR-0025, #141/M1-4), reached through the main-thread wasm seam this
// module now wraps — `availableActions`, `canMarkDone`, `canGrill` and
// `grillButtonLabel` below are thin re-exports over `../decisions/seam`,
// kept as named functions here (rather than deleted) so every caller
// (`ItemPanel`, `TriageRow`, `LedgerScreen`, `FrontierColumns`,
// `NowScreen`, `MarkDoneButton`) and `item-actions.test.ts` stay untouched —
// the unchanged component tests are the regression proof the sink was a
// rewire, not a rewrite.
//
// **What did NOT move.** `applyItemAction` and `resolveFallbackPending`
// are view-model plumbing over `TaskItemDTO`, not decisions two clients
// could disagree about: `applyItemAction`'s `Date.now()`/`archivedAt` write
// and `resolveFallbackPending`'s optimistic-vs-live reconciliation are
// screen-local state management, and ADR-0025's own module doc names
// clock-freedom as the line a decision has to cross to sink. `applyItemAction`
// still asks the seam which stage an action resolves to
// (`appliedStage`) rather than holding a second copy of that mapping.
//
// A Grill is still not an `ItemAction`: it opens the interview takeover
// rather than mutating the item directly, and its eventual stage move comes
// from `hummingbird-domain`'s own verdict function (`resulting_stage`),
// never from this file's mapping — `canGrill` only decides whether the
// button is offered.
import type { TaskActionName, TaskItemDTO, TaskStageName } from "../store/protocol";
import {
  appliedStage,
  availableActions as seamAvailableActions,
  canGrill as seamCanGrill,
  canMarkDone as seamCanMarkDone,
  grillButtonLabel as seamGrillButtonLabel,
} from "../decisions/seam";

export function availableActions(stage: TaskStageName): readonly TaskActionName[] {
  return seamAvailableActions(stage);
}

/** Whether a row offers the one-click "mark done" checkmark: any live,
 * unarchived stage but Done itself. Deliberately WIDER than
 * `availableActions` — Triage and Grilling stay pre-action in the detail
 * panel's vocabulary (no start/block there), but a capture that turned out
 * already finished is still one click, which is the amendment the checkmark
 * decision made to "pre-action by definition". The one deciding function for
 * every screen's row, so the affordance cannot drift between them. */
export function canMarkDone(item: Pick<TaskItemDTO, "stage" | "archivedAt">): boolean {
  return seamCanMarkDone(item.stage, item.archivedAt !== null);
}

/** Whether a row offers "Grill me" (#355, ADR-0023; widened to Now's
 * frontier by #359): Triage, Grilling, Ready and In Progress. Since #360,
 * promoting an item straight into Grilling is no longer a triage gesture at
 * all — Grill is the only way an item gets there — which is also why
 * Grilling itself stays `true`: it has nothing further to grill until this
 * transcript resolves, but #357's combined queue can still show a Grilling
 * row on Now, and its "Resume grill" button must read the same here as on
 * Triage. Blocked and Done are not reachable from either screen's row, so
 * this slice opens no such button for them. */
export function canGrill(stage: TaskStageName): boolean {
  return seamCanGrill(stage);
}

/** The Grill button's own label (#356, ADR-0023): "Resume grill" when this
 * item already carries a draft, "Grill me" otherwise — decided by this ONE
 * function rather than a per-screen branch on `hasDraft`, the same "one
 * deciding function" discipline `canGrill`/`canMarkDone` document for their
 * own affordance. */
export function grillButtonLabel(hasDraft: boolean): "Grill me" | "Resume grill" {
  return seamGrillButtonLabel(hasDraft);
}

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
 * can execute it; `NowScreen.tsx` only threads React state through it.
 *
 * **Not a sunk decision** (ADR-0025/#141/M1-4 scope note): this is optimistic
 * UI reconciliation over screen-local state, not a fact two clients could
 * disagree about — it stays a plain TS function. */
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

/** Mirrors the stage `hummingbird_core::decisions::applied_stage` (the same
 * closed action-to-stage vocabulary `hummingbird_core::ItemAction::stage`
 * already states once) resolves for `action` — restated here only for the
 * UI's own optimistic display (`NowScreen.tsx`'s `RealFrontier`).
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
  // `"cancel"` is checked by name, never inferred from `appliedStage`
  // answering `null` — the seam's `item_applied_stage` answers `null` for
  // BOTH `"cancel"` (no stage, `archivedAt` is the write) and an
  // unrecognised action string (the wasm side rejects it before the seam,
  // exactly as `ItemAction::parse` does in `client/core/src/lib.rs`). Those
  // are not the same outcome: the old switch-based implementation had no
  // default case at all (`TaskActionName` is closed), so an action outside
  // the vocabulary never reached a write. Collapsing both `null`s onto the
  // archive branch would silently set `archivedAt` for a value this
  // function does not recognise — the destructive default this review
  // comment exists to remove.
  if (action === "cancel") {
    // `hummingbird_domain::Item::archived_at` is `ms epoch | null` — any
    // non-null value marks it archived; the exact timestamp is never
    // read back from this projection (`Core::act`'s own enqueue call
    // carries the real one).
    return { ...item, archivedAt: Date.now(), pending: true };
  }
  const stage = appliedStage(action);
  if (stage === null) {
    // Defensive only: `TaskActionName` is a closed TS union, so this path
    // is unreachable through a well-typed caller. If it is ever reached
    // anyway, the item is returned unmutated rather than guessing at a
    // write — the same "reject before the seam" discipline the wasm
    // boundary itself applies to an unrecognised action string.
    return item;
  }
  return { ...item, stage, pending: true };
}
