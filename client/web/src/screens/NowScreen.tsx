import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Card } from "../components/core/Card";
import { ItemPanel } from "../components/domain/ItemPanel";
import { ItemRow } from "../components/domain/ItemRow";
import { EmptyState } from "../components/feedback/EmptyState";
import type { GrillTakeoverWiring } from "../shell/useGrillTakeoverWiring";
import type { Screen } from "../shell/screens";
import type { MicrotaskWiring } from "../shell/useMicrotaskWiring";
import type {
  CalendarReadDTO,
  TaskActionName,
  TaskItemDTO,
} from "../store/protocol";
import type { TaskState } from "../store/store";
import type { TriageEdits } from "../store/worker-client";
import { blockedReasonLabel } from "./blocked-reason";
import { FrontierColumns } from "./FrontierColumns";
import { GrillTakeover } from "./GrillTakeover";
import { applyItemAction, canMarkDone, resolveFallbackPending } from "./item-actions";
import { Aside, Column, Section, TwoColumn } from "./layout";
import type { QuestionInputs } from "./questions/contract";
import { RankedRegion } from "./questions/RankedRegion";
import type { StorageLike } from "./storage";
import {
  actFailureFor,
  grillCompletionFailureFor,
  strandedActFailure,
  strandedTriageFailure,
} from "./write-failure";
import { TriageRow } from "./TriageRow";

/** The DOM id Now's own "Grill me"/"Resume grill" button carries (#359) — the
 * same "look it up by id rather than hold a ref across an unmount" contract
 * `TriageRow.tsx`'s `grillMeButtonId` uses, since the takeover unmounts the
 * whole board and the button Back has to refocus is a NEW element by the time
 * it remounts. A distinct namespace from Triage's: the two screens are never
 * mounted at once, but the id still ought to say which surface it is. */
export function nowGrillMeButtonId(itemId: string): string {
  return `now-grill-me-${itemId}`;
}

export interface NowScreenProps {
  onScreen: (screen: Screen) => void;
  /** S10's real frontier data (issue #108). */
  task: TaskState;
  /** `useSyncWiring.ts`'s unconditional 30s tick — coarse enough for
   * urgency's own bucket sizes (`urgency.ts`) and for the ranked region's
   * bands, and the ONE clock this screen gets. (It used to be
   * `useCalendarWiring`'s own tick, whose only honest consumer was the
   * context tile ADR-0015 replaced; that timer is gone.) */
  nowMs: number;
  selectedItemId: string | null;
  onOpenItem: (itemId: string) => void;
  onCloseItemDetail: () => void;
  /** S11/#109's act affordances (start/complete/block/cancel), forwarded to
   * whichever item is currently open in detail. */
  onAct: (itemId: string, action: TaskActionName) => void;
  /** Issue #267's calendar-events arm — `CalendarState.eventReads`, threaded
   * through exactly as `task.paneReads` is: the region's `QuestionInputs`
   * needs a real value here, not a literal `{}`, or every calendar-lane
   * question #122 goes on to register renders as though nothing was ever
   * requested. */
  calendarReads: Record<string, CalendarReadDTO | undefined>;
  /** #122 review fix: `CalendarState.connected` (`store/store.ts`), threaded
   * straight through — the fact that separates "never set up" from "no
   * snapshot yet" for every calendar-lane question's `QuestionInputs`. */
  calendarConnected: boolean;
  /** #122's do-date write affordance — forwarded straight to `RankedRegion`. */
  onSetScheduledDate?: (itemId: string, date: string | null) => void;
  /** #273's microtask affordance for the open item, forwarded straight to
   * `ItemDetailPanel`. */
  microtask?: MicrotaskWiring;
  /** S13/#111's triage mutation, for the captures now sitting in the frontier's
   * own columns — `shell/useTriageWiring.ts`'s `triage`, the SAME callback the
   * Triage screen gets. Now is a second view of one inbox, never a second entry
   * point into it. */
  onTriage?: (
    itemId: string,
    destination: "ready" | null,
    edits: TriageEdits,
  ) => void;
  /** #631's inline "new project" affordance, threaded to Now's own forced-open
   * `TriageRow` the same way `TriageScreen` threads it to its rows (#652) —
   * the SAME `handleCreateProject` `App.tsx` gives Triage, so a project
   * created from either surface goes through the one write door. Optional
   * for the same "no worker, no affordance" reason `onTriage` is. */
  onCreateProject?: (name: string) => void;
  /** Injected storage for this screen's device-local view preferences —
   * resolved once here rather than read in each consumer: `RankedRegion`'s pane
   * overrides, and (#403) the frontier's grouping axis and collapsed columns.
   * One storage seam, two readers. */
  storage?: StorageLike;
  /** Is the standing-questions aside shut? Owned by `App.tsx`, not here: the
   * control that opens and shuts it is a single button in the shell's header,
   * which is also where persistence lives (`questions/aside-prefs.ts`). This
   * screen only renders the answer. */
  asideCollapsed?: boolean;
  /** "Grill me" reaches Now (#359, ADR-0023): the same whole-composite
   * `GrillTakeoverWiring` `TriageScreen` gets, from the same one instance
   * `App.tsx` owns — there is exactly one interview session for the whole
   * app, never a second one per screen. */
  grill?: GrillTakeoverWiring;
}

/** The real-data half of `QuestionInputs` (never demo's) — exported so a
 * component test can drive the exact object this screen threads to
 * `RankedRegion` through a genuinely mounted consumer, proving delivery
 * rather than merely inspecting the store snapshot (#267's review point).
 *
 * `items` (#122, widened at #675) is every live item this device knows
 * about: `task.frontier`, `task.blocked`'s items, `task.triageInbox`,
 * `task.grillingItems` and `task.externallyBlocked`. The first two are
 * `RealFrontier`'s own `allItems`, computed a second time here because that
 * helper is scoped to `RealFrontier`'s render and this function has to stay
 * callable standalone by both `NowScreen` and its own tests.
 *
 * The last three arrived with the homework pane, whose subject is the
 * operator's items and whose reading of "open" is everything not Done — a
 * captured piece of homework is still homework, and so is one waiting on a
 * callback. The first two of the three were already in `TaskState`;
 * `externallyBlocked` was not, and needed a query of its own
 * (`Core::externally_blocked`, `getExternallyBlocked`) because
 * `task.blocked` is the *relation* blockers and holds only Ready/InProgress
 * items — so a `Stage::Blocked` item was reachable from no list this screen
 * held and disappeared from the pane, taking the right answer with it.
 *
 * **The weekend pane was pinned against this widening first**
 * (`weekend.rs`'s `MERGED_STAGES`, its own commit): it merges items onto
 * calendar days and would otherwise have started showing Triage items the
 * moment this line changed. A question added later that reads `items` owes
 * the same explicit filter — the list here is "every live item", never
 * "the items your pane should consider". */
export function realQuestionInputs(
  task: TaskState,
  calendarReads: Record<string, CalendarReadDTO | undefined>,
  calendarConnected: boolean,
): Omit<QuestionInputs, "nowMs"> {
  return {
    sync: {
      latestOutcome: task.lastSyncOutcome,
      latestInformativeAtMs: task.lastSyncAtMs,
      lastSuccessfulAtMs: task.lastSuccessfulSyncAtMs,
    },
    bindings: task.bindings,
    paneReads: task.paneReads,
    calendarReads,
    calendarConnected,
    items: [
      ...task.frontier,
      ...task.blocked.map((entry) => entry.item),
      ...task.triageInbox,
      ...task.grillingItems,
      ...task.externallyBlocked,
    ],
  };
}

/** The slot the selected item expands into, above the columns (#404).
 *
 * Its whole job beyond layout is the scroll: a card near the bottom of a long
 * board would otherwise expand off-screen, which makes "it goes to the top"
 * true of the DOM and false for the reader. Keyed by the caller on the item id,
 * so this mounts fresh per selection and the effect fires once per item rather
 * than on every re-render of the same one.
 *
 * `scrollIntoView` is called defensively: jsdom does not implement it, and a
 * view preference — which is all this is — is never worth a crash. */
function SelectedItemSection({ children }: { children: ReactNode }) {
  const slot = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    slot.current?.scrollIntoView?.({ block: "nearest" });
  }, []);
  return <div ref={slot}>{children}</div>;
}

/** Real-data frontier/blocked rendering (issue #108) — derives everything at
 * read time from the `TaskItemDTO`s the store actually holds. The kit
 * world's hand-authored fixture render (`?demo=kit`'s hero card and "Also
 * startable" list) was retired at #456: this is the screen's only render
 * path now. */
function RealFrontier({
  task,
  nowMs,
  selectedItemId,
  onOpenItem,
  onCloseItemDetail,
  onAct,
  microtask,
  onTriage,
  onCreateProject,
  storage,
  grill,
}: Pick<
  NowScreenProps,
  | "task"
  | "nowMs"
  | "selectedItemId"
  | "onOpenItem"
  | "onCloseItemDetail"
  | "onAct"
  | "microtask"
  | "onTriage"
  | "onCreateProject"
  | "storage"
  | "grill"
>) {
  const allItems = [...task.frontier, ...task.blocked.map((entry) => entry.item)];
  const liveSelectedItem = selectedItemId
    ? (allItems.find((item) => item.id === selectedItemId) ?? null)
    : null;

  // The selected card may now be an unsorted capture rather than a startable
  // action, because both live in the same columns. A capture's editor is
  // `TriageRow`'s, never `ItemDetailPanel`'s: the detail panel's act
  // vocabulary offers a capture nothing at all (`item-actions.ts` —
  // "Triage and Grilling are pre-action by definition"), so opening one there
  // would be a panel of absent buttons over an item whose one real affordance
  // is being sorted. Selecting either kind fills the SAME slot above the
  // columns, so ADR-0021 decision 7's "selecting a card is not a takeover" now
  // covers captures too, and S13/#111's "two editors are never open at once"
  // survives for free — the slot holds one thing.
  // #357: a Grilling-stage item lands in the same columns as a Triage one
  // (`triageProcessQueue`), so it is just as much a "capture" for selection
  // purposes here — same `TriageRow` editor, same pre-action vocabulary.
  const selectedCapture = selectedItemId
    ? ([...task.triageInbox, ...task.grillingItems].find((item) => item.id === selectedItemId) ??
      null)
    : null;

  // Reviewer finding on PR #207: a failed `actResult` used to be recorded in
  // `TaskState.lastAct` and rendered nowhere — this is what makes it visible,
  // matched to the currently open item by id so a stale failure from a
  // DIFFERENT item never bleeds into this one.
  const actError = selectedItemId ? actFailureFor(task.lastAct, selectedItemId) : null;

  // #418's twin, on the other mutation. `actError` above only renders inside
  // `ItemDetailPanel`, so an act that failed after the reader closed the panel
  // was displayed nowhere at all — the same defect the amendment fixed for
  // triage, and the reason this pair of lines exists rather than one.
  //
  // The panel is open only when a *non-capture* fills the slot: a selected
  // capture gets `TriageRow`, which speaks for triage failures and not for act
  // ones, though its checkmark issues an act (`canMarkDone`). So the id passed
  // as "the panel's item" is the selected one only on that branch, and the
  // name is looked up across the inbox too, since a capture can be what
  // failed.
  const strandedAct = strandedActFailure(
    task.lastAct,
    selectedCapture ? null : selectedItemId,
    [...allItems, ...task.triageInbox, ...task.grillingItems],
  );

  // S11/#109's item detail panel must stay open (reviewer finding on PR
  // #207) even after an act moves the item somewhere neither `frontier`
  // nor `blocked` lists — `"block"` sets `Stage::Blocked`, which is outside
  // both queries by design (S10's own scope: neither reads a Blocked-stage
  // item at all), so `liveSelectedItem` above goes `null` the instant a
  // block succeeds even though the panel — and its "Start"/"Cancel" row
  // (`availableActions("blocked")`) — should stay showing AND become
  // clickable once the mutation drains. `optimisticItem` is the fallback:
  // `applyItemAction` mirrors the same action->stage mapping `Core::act`
  // itself applies, so the panel shows the real post-action state
  // immediately rather than either freezing on stale pre-action data or
  // going blank. Its frozen `pending: true` is NOT what renders, though —
  // round 2 of PR #207's review found that frozen flag kept the row
  // disabled forever. The rendered `pending` comes from
  // `resolveFallbackPending` over the LIVE `task.pending[id]` (fed by
  // `worker-client.ts` on every ok act and by `useItemDetailWiring` per
  // sync cycle), so the row enables the moment the queued mutation
  // confirms. Cleared whenever `selectedItemId` itself changes (a
  // different item opened, or the panel closed) so a stale optimistic item
  // from a PREVIOUS selection can never leak into a new one.
  const [optimisticItem, setOptimisticItem] = useState<TaskItemDTO | null>(null);
  // True from an act click until the live `isPending` read confirms that
  // act queued — see `resolveFallbackPending`'s doc for the stale-`false`
  // window this bridges.
  const [awaitingPendingConfirm, setAwaitingPendingConfirm] = useState(false);
  // The React-docs "adjusting state when a prop changes" pattern — `setState`
  // called during render, guarded by comparing against state (never a ref;
  // this repo's lint config's `react-hooks/refs` forbids reading/writing a
  // ref during render, and `react-hooks/set-state-in-effect` forbids the
  // `useEffect` version of this same adjustment). React bails out of
  // re-rendering with the stale props immediately when it sees a `setState`
  // call during render, so this clears the stale optimistic item in the
  // same render `selectedItemId` changed in, not a follow-up one.
  const [lastSelectedItemId, setLastSelectedItemId] = useState(selectedItemId);
  if (selectedItemId !== lastSelectedItemId) {
    setLastSelectedItemId(selectedItemId);
    if (optimisticItem !== null) {
      setOptimisticItem(null);
    }
    if (awaitingPendingConfirm) {
      setAwaitingPendingConfirm(false);
    }
  }

  const fallbackItem =
    optimisticItem && optimisticItem.id === selectedItemId ? optimisticItem : null;
  const fallbackResolution = fallbackItem
    ? resolveFallbackPending(
        fallbackItem.pending,
        task.pending[fallbackItem.id],
        awaitingPendingConfirm,
      )
    : null;
  // Same guarded setState-during-render pattern as `lastSelectedItemId`
  // above: the confirm flag clears in the render that observes the live
  // `true`, never via an effect.
  if (fallbackResolution && fallbackResolution.awaitingConfirm !== awaitingPendingConfirm) {
    setAwaitingPendingConfirm(fallbackResolution.awaitingConfirm);
  }

  const selectedItem =
    liveSelectedItem ??
    (fallbackItem && fallbackResolution
      ? { ...fallbackItem, pending: fallbackResolution.pending }
      : null);

  // #418. `TriageRow` renders its own failure outside its expanded block so a
  // late result still lands on a collapsed row — true on Triage, where the
  // rows stay mounted in a list, and false here from the moment the row became
  // the slot: closing the slot unmounts it outright. This is where the failure
  // goes when there is no row left to wear it. `strandedTriageFailure` is
  // silent while the failing capture IS the open one, so the two surfaces
  // never both speak for one result.
  //
  // Sits below `selectedItem` because it must read the *resolved* one. Either
  // editor counts as an owner: the capture's `TriageRow`, or a minted item's
  // `ItemPanel`, which says its own triage failures since it gained an Edit
  // mode — but only when that editor actually rendered. `selectedItemId` alone
  // would be the wrong owner: `selectedCapture` is looked up BY it, so the
  // expression would collapse to `selectedItemId` and claim an owner in the one
  // case where there is none — an id selected whose item has left both frontier
  // and blocked with no optimistic fallback left, where the slot renders
  // nothing and this line is the only surface the failure has.
  const strandedTriage = strandedTriageFailure(
    task.lastTriage,
    selectedCapture?.id ?? selectedItem?.id ?? null,
    [...task.triageInbox, ...task.grillingItems],
  );

  // #359: Grill reaches Now. Back restores focus to the "Grill me" button the
  // reader pressed — never a held DOM reference (the takeover unmounts the
  // whole centre column, so that exact button is gone by the time Back is
  // pressed), looked up by id instead, the same contract `TriageScreen.tsx`
  // uses for its own row.
  const focusOnCloseRef = useRef<string | null>(null);

  function handleGrillMe(itemId: string): void {
    focusOnCloseRef.current = itemId;
    grill?.open(itemId);
  }

  function handleGrillBack(): void {
    grill?.back();
  }

  useEffect(() => {
    if (grill?.openItemId !== null && grill?.openItemId !== undefined) {
      return;
    }
    const itemId = focusOnCloseRef.current;
    if (itemId === null) {
      return;
    }
    focusOnCloseRef.current = null;
    // Review round 1's non-blocking note: a `fog_remains` confirm moves this
    // item into `task.grillingItems`, so the slot that reopens is the
    // `selectedCapture`/`TriageRow` branch below, which carries no
    // `nowGrillMeButtonId` — this lookup no-ops and focus drops to `<body>`.
    // Triage's own version of this effect has the identical shape, so this
    // is an inherited gap, not a regression this slice introduced.
    document.getElementById(nowGrillMeButtonId(itemId))?.focus();
  }, [grill?.openItemId]);

  // Resolved against every item this screen knows about — the frontier and
  // blocked items detail mode opens, plus the captures `TriageRow` opens —
  // the same "the takeover's item must resolve against the combined set, not
  // just one array" fix #357 made for `TriageScreen.tsx`'s own `openItem`.
  // `grill.openItemId` is app-wide (`shell/useGrillTakeoverWiring.ts` — one
  // interview session for the whole app), so a grill opened from Triage and
  // then navigated to renders here too, over Now's centre column: intended,
  // not a bug, since it is the same one session either screen would resume.
  const openItem = grill?.openItemId
    ? [...allItems, ...task.triageInbox, ...task.grillingItems].find(
        (item) => item.id === grill.openItemId,
      )
    : undefined;

  // The takeover replaces the centre column ONLY — never the standing-
  // questions aside beside it, which sits in `NowScreen`'s own `TwoColumn`,
  // a sibling of this component's own render rather than something nested
  // inside it. That is the one thing this surface has that Triage's
  // `SingleColumn` takeover does not, and #359 calls it out explicitly:
  // preserving it is not a side effect of this early return, it is the
  // reason the return stops here instead of one level up.
  if (grill && openItem) {
    return (
      <GrillTakeover
        item={openItem}
        steps={grill.sessionSteps}
        turn={grill.turn}
        turns={grill.turns}
        backLabel="Back to Now"
        onAnswer={grill.answer}
        onKeepGrilling={grill.keepGrilling}
        onRetry={grill.retry}
        onConfirm={grill.confirm}
        onBack={handleGrillBack}
        onDiscard={grill.discard}
        completionError={grillCompletionFailureFor(task.lastGrillCompletion, grill.confirmSeed)}
      />
    );
  }

  return (
    <>
      {/* Above the slot, not inside it: these lines exist precisely for the
          renders where the slot is empty. Text and nothing else — ADR-0021
          decision 2 keeps colour on a card meaning urgency and nothing else,
          so a failure states itself in words, which is the same accessibility
          argument the cards already make for urgency. `role="alert"`, like
          every other danger paragraph in this app: it appears with no other
          change on the page, so colour alone would never reach a screen
          reader.

          Two, because triage and act are two results the store holds at once
          (`TaskState.lastTriage` and `lastAct` are separate fields), and a
          failure of each can be stranded at the same time. Each is silent
          whenever the editor that owns it is the thing on screen, so no
          failure is ever stated twice. */}
      {(
        [
          ["triage", strandedTriage],
          ["act", strandedAct],
        ] as const
      ).map(([kind, message]) =>
        message ? (
          <p
            key={kind}
            role="alert"
            style={{
              font: "var(--type-body-sm)",
              color: "var(--status-danger-fg)",
              margin: 0,
            }}
          >
            {message}
          </p>
        ) : null,
      )}

      {/* #404 / ADR-0021 decision 7: the selected item expands ABOVE the
          columns, which stay standing under it — this used to be an early
          `return` of the panel *instead of* the frontier, so picking one action
          cost you the view of everything you might have picked instead. The
          slot above the frontier is Now's own: ADR-0015 gives the *aside* to
          the ranked region, and its "standing questions never take the banner"
          is a claim about the aside's contents, not about this column.

          The panel is the existing `ItemDetailPanel`, threaded the app's own
          act callback, steps, last-act error and microtask affordance — never a
          second implementation, so whatever lands on item detail next (Grill
          me, #359) arrives with no parallel code path to reconcile. */}
      {selectedCapture ? (
        <SelectedItemSection key={selectedCapture.id}>
          {/* The same `TriageRow` the Triage screen renders, forced open —
              never a second editor written for this surface. Its collapsed
              header is the row's own close control, so the slot needs no
              chrome of its own. */}
          <TriageRow
            key={selectedCapture.id}
            item={selectedCapture}
            projects={task.projects ?? []}
            expanded
            onToggle={onCloseItemDetail}
            nowMs={nowMs}
            onTriage={onTriage}
            onComplete={(itemId) => onAct(itemId, "complete")}
            lastTriage={task.lastTriage}
            onCreateProject={onCreateProject}
            lastProjectWrite={task.lastProjectWrite}
          />
        </SelectedItemSection>
      ) : selectedItem ? (
        <SelectedItemSection key={selectedItem.id}>
          <ItemPanel
            // Remounts per item so the grain select and the Edit state reset
            // with it — a grain chosen for one item says nothing about the
            // next, and neither does a half-typed edit.
            key={selectedItem.id}
            mode="detail"
            item={selectedItem}
            projects={task.projects ?? []}
            steps={task.stepsByItem[selectedItem.id] ?? []}
            onClose={onCloseItemDetail}
            onAct={(action) => {
              setOptimisticItem(applyItemAction(selectedItem, action));
              setAwaitingPendingConfirm(true);
              onAct(selectedItem.id, action);
            }}
            actError={actError}
            // Edit mode's Save — `Core::triage` with no destination (#122), so
            // editing a minted action leaves its stage exactly where it is.
            onTriage={onTriage}
            lastTriage={task.lastTriage}
            microtask={microtask}
            // #359: "Grill me" reaches Now — gated by `item-actions.ts`'s
            // `canGrill`, the one deciding function, same as Triage's.
            onGrillMe={grill ? handleGrillMe : undefined}
            hasGrillDraft={task.grillDraftItemIds.includes(selectedItem.id)}
            grillMeId={nowGrillMeButtonId(selectedItem.id)}
          />
        </SelectedItemSection>
      ) : null}

      {/* The empty frontier is a *section* rather than a whole-branch return:
          an inbox full of captures with nothing yet promoted is the commonest
          state of a new device, and returning early here would render
          "Nothing to start" over the one thing worth showing.

          The captures are now cards in those same columns, so the condition
          counts them: a board carrying nothing but unsorted captures is
          emphatically not "nothing to start" — it says exactly what to do next,
          which is sort them.

          Withheld while the slot above is filled, which #404 made a reachable
          state rather than a hypothetical one: block or cancel your only
          startable item and both queries go empty while the optimistic fallback
          keeps the panel standing, so without this guard the screen would show
          the open item and "Nothing to start" underneath it. "Nothing to start"
          is a claim about what you could pick *instead*, and that is not the
          question being asked while one item is expanded. */}
      {selectedItem === null &&
      selectedCapture === null &&
      task.frontier.length === 0 &&
      task.blocked.length === 0 &&
      task.triageInbox.length === 0 &&
      task.grillingItems.length === 0 ? (
        <Card padding="var(--space-3)">
          <EmptyState
            icon="zap"
            headingLevel={2}
            title="Nothing to start"
            body="No actions are Ready or In Progress right now."
          />
        </Card>
      ) : null}

      {/* ADR-0021: the frontier in columns, grouped by a switchable axis, in
          place of the fixed project sections this branch used to cut. Project
          is now one of the four axes, so nothing is lost.

          The unsorted captures go in as well, marked with their stage chip and
          ordered under the startable actions of whichever column they land in
          — Now no longer has a triage section of its own, so an inbox with an
          empty frontier still renders the board. */}
      {task.frontier.length > 0 || task.triageInbox.length > 0 || task.grillingItems.length > 0 ? (
        <FrontierColumns
          frontier={task.frontier}
          triage={task.triageInbox}
          grilling={task.grillingItems}
          draftItemIds={task.grillDraftItemIds}
          projects={task.projects ?? []}
          nowMs={nowMs}
          selectedItemId={selectedItemId}
          onOpenItem={onOpenItem}
          onAct={onAct}
          storage={storage}
        />
      ) : null}

      {task.blocked.length > 0 ? (
        <Section
          title="Blocked"
          meta={`${task.blocked.length} ${task.blocked.length === 1 ? "action" : "actions"}`}
        >
          <Card padding="var(--space-3)">
            {task.blocked.map((entry) => (
              // This wrapper is the ONE dimming source for a blocked row —
              // `ItemRow`'s own `pending` indicator is a chip, never an
              // opacity change (see that component), specifically so
              // stacking the two here can never compound into an
              // over-muted row for something both blocked and pending
              // (PR #200 review).
              <div key={entry.item.id} style={{ opacity: 0.6 }}>
                <ItemRow
                  title={entry.item.title}
                  stage={entry.item.stage}
                  size={entry.item.size}
                  energy={entry.item.energy}
                  priority={entry.item.priority}
                  pending={entry.item.pending}
                  onComplete={
                    canMarkDone(entry.item)
                      ? () => onAct(entry.item.id, "complete")
                      : undefined
                  }
                />
                <span
                  className="hb-meta"
                  style={{
                    display: "block",
                    padding: "0 var(--space-5) var(--space-3)",
                    color: "var(--status-danger-fg)",
                  }}
                >
                  {blockedReasonLabel(entry.blockedBy.map((blocker) => blocker.title))}
                </span>
              </div>
            ))}
          </Card>
        </Section>
      ) : null}

    </>
  );
}

export function NowScreen({
  onScreen,
  task,
  nowMs,
  selectedItemId,
  onOpenItem,
  onCloseItemDetail,
  onAct,
  calendarReads,
  calendarConnected,
  onSetScheduledDate,
  microtask,
  onTriage,
  onCreateProject,
  storage,
  asideCollapsed = false,
  grill,
}: NowScreenProps) {
  // Resolved once, for both the ranked region and the triage section. The
  // fallback keeps every existing caller (and every test that mounts this
  // screen without the prop) on exactly the storage it had before.
  const resolvedStorage =
    storage ?? (typeof localStorage === "undefined" ? undefined : localStorage);

  return (
    <TwoColumn>
      <Column>
        <RealFrontier
          task={task}
          nowMs={nowMs}
          selectedItemId={selectedItemId}
          onOpenItem={onOpenItem}
          onCloseItemDetail={onCloseItemDetail}
          onAct={onAct}
          microtask={microtask}
          onTriage={onTriage}
          onCreateProject={onCreateProject}
          storage={resolvedStorage}
          grill={grill}
        />
      </Column>

      {/* Shut, the panel is gone from the screen entirely rather than shrunk
          to a strip: the `?` that reopens it lives in the shell's header
          (`Header.tsx`, between Refresh and New), so nothing has to stay
          behind to hold a control. That is also why the landmark unmounts —
          an `aside` named "Standing questions" containing nothing is a
          landmark that lies to a screen reader. */}
      {asideCollapsed ? null : (
        <Aside label="Standing questions">
          {/* No heading and no control of its own. The words "Standing
              questions" sat above a region that says what it is on every card,
              and the landmark's `aria-label` is where that name belongs for
              anyone who cannot see the panel. The collapse control is the
              header's, in both directions (`shell/Header.tsx`) — a toggle that
              lives inside the thing it hides has to move when you press it,
              and then you have to find it twice. */}

          {/* ADR-0015's ranked region replaces everything that used to be in
              here — the context tile, the demo standing-question card and the
              snapshot tiles.

              The landmark was still called `Context` long after that swap,
              which is what ADR-0021 renamed (#401): the panel holds standing
              questions and nothing called context, this was the one inaccurate
              aside name of the four, and the word is needed for the frontier's
              grouping axis in the centre column — otherwise the screen says
              "Context" twice, meaning an item's `@computer` on one side and
              context *sources* on the other. */}
          <RankedRegion
            surface="now"
            inputs={realQuestionInputs(task, calendarReads, calendarConnected)}
            nowMs={nowMs}
            syncOutcomeSeq={task.syncOutcomeSeq}
            storage={resolvedStorage}
            onScreen={onScreen}
            onSetScheduledDate={onSetScheduledDate}
          />
        </Aside>
      )}
    </TwoColumn>
  );
}
