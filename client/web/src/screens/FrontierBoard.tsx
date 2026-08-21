// The frontier board: the whole of a surface's centre column — the selected
// item's slot, the frontier in columns (`FrontierColumns.tsx`) and the Blocked
// section under them — plus the render-phase optimistic fallback that keeps
// item detail standing when an act moves the item out of both queries.
//
// Extracted from `NowScreen.tsx` at the point a second surface needed it: a
// project's dossier renders this same board filtered to that project's items
// (ADR-0030's project lane, ADR-0021's board). Two things vary between the two
// callers and nothing else does — the four queries, which the caller slices,
// and `screen`/`axes`, which say whose view preferences to use and which axis
// buttons to offer. The filter is a TS re-slice of already-decided data, not a
// decision: ADR-0025 puts ordering, grouping and faceting behind the seam, and
// an identity comparison against a stored `projectId` is none of those.
//
// Now keeps what is genuinely Now's: the standing-questions aside beside this
// board, and the `grill` wiring — see `NowScreen.tsx`.

import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Card } from "../components/core/Card";
import { ItemPanel } from "../components/domain/ItemPanel";
import { ItemRow } from "../components/domain/ItemRow";
import { EmptyState } from "../components/feedback/EmptyState";
import type { GrillTakeoverWiring } from "../shell/useGrillTakeoverWiring";
import type { MicrotaskWiring } from "../shell/useMicrotaskWiring";
import type {
  BlockedFrontierEntryDTO,
  TaskActionName,
  TaskItemDTO,
} from "../store/protocol";
import type { TaskState } from "../store/store";
import type { TriageEdits } from "../store/worker-client";
import { blockedReasonLabel } from "./blocked-reason";
import { FrontierColumns } from "./FrontierColumns";
import { type FrontierAxis } from "./frontier-columns";
import type { FrontierPrefsScreen } from "./frontier-prefs";
import { GrillTakeover } from "./GrillTakeover";
import { applyItemAction, canMarkDone, resolveFallbackPending } from "./item-actions";
import { Section } from "./layout";
import type { StorageLike } from "./storage";
import {
  actFailureFor,
  grillCompletionFailureFor,
  strandedActFailure,
  strandedTriageFailure,
} from "./write-failure";
import { TriageRow } from "./TriageRow";

/** Now's copy, and the default: this board's whole point is "what can I start",
 * so with nothing in it that is what it reports. */
const DEFAULT_EMPTY_STATE = {
  title: "Nothing to start",
  body: "No actions are Ready or In Progress right now.",
} as const;

/** The DOM id Now's own "Grill me"/"Resume grill" button carries (#359) — the
 * same "look it up by id rather than hold a ref across an unmount" contract
 * `TriageRow.tsx`'s `grillMeButtonId` uses, since the takeover unmounts the
 * whole board and the button Back has to refocus is a NEW element by the time
 * it remounts. A distinct namespace from Triage's: the two screens are never
 * mounted at once, but the id still ought to say which surface it is.
 *
 * Still `now-`-prefixed now that the board is on two surfaces: only Now
 * threads `grill`, so this id is minted on that surface and nowhere else. */
export function nowGrillMeButtonId(itemId: string): string {
  return `now-grill-me-${itemId}`;
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
 * path now.
 *
 * **The board itself, not one screen's version of it.** Extracted from
 * `NowScreen.tsx` when the project dossier's centre column became the same
 * board filtered to one project's items — the optimistic-fallback dance
 * below, the capture-vs-action slot branch and the Blocked section are the
 * whole reason that surface composes this rather than `FrontierColumns`
 * directly, which would have been the second implementation ADR-0021
 * forbids.
 *
 * The four queries arrive as explicit props rather than being read off
 * `task`, which is what lets a caller pass a filtered slice; `task` itself
 * stays for the broadcast slots (`lastAct`, `lastTriage`, `pending`,
 * `stepsByItem`, `grillDraftItemIds`, `projects`, `lastGrillCompletion`,
 * `lastProjectWrite`) — app-wide facts, correct on either surface. */
export function FrontierBoard({
  task,
  frontier,
  triage,
  grilling,
  blocked,
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
  screen,
  axes,
  emptyState = DEFAULT_EMPTY_STATE,
}: {
  /** The store's broadcast slots — never the four queries, which are the
   * props below so a caller can filter them. */
  task: TaskState;
  /** `TaskState.frontier`, or a caller's slice of it. */
  frontier: readonly TaskItemDTO[];
  /** `TaskState.triageInbox`, or a caller's slice of it. */
  triage: readonly TaskItemDTO[];
  /** `TaskState.grillingItems`, or a caller's slice of it. */
  grilling: readonly TaskItemDTO[];
  /** `TaskState.blocked`, or a caller's slice of it. */
  blocked: readonly BlockedFrontierEntryDTO[];
  nowMs: number;
  selectedItemId: string | null;
  onOpenItem: (itemId: string) => void;
  onCloseItemDetail: () => void;
  onAct: (itemId: string, action: TaskActionName) => void;
  microtask?: MicrotaskWiring;
  onTriage?: (itemId: string, destination: "ready" | null, edits: TriageEdits) => void;
  onCreateProject?: (name: string) => void;
  storage?: StorageLike;
  /** Only Now passes this: the project board renders no "Grill me" button, so
   * no takeover is ever reachable there (`ItemPanel` omits the button when the
   * callback is absent). */
  grill?: GrillTakeoverWiring;
  /** Forwarded to `FrontierColumns` — whose preference keys this board uses. */
  screen: FrontierPrefsScreen;
  /** Forwarded to `FrontierColumns` — which switcher buttons it offers. */
  axes?: readonly FrontierAxis[];
  /** What the board says when it holds nothing at all. Overridable because
   * "Nothing to start" is a claim about the whole frontier, and on a
   * project's board it would be a false one. */
  emptyState?: { title: string; body: string };
}) {
  const allItems = [...frontier, ...blocked.map((entry) => entry.item)];
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
    ? ([...triage, ...grilling].find((item) => item.id === selectedItemId) ??
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
    [...allItems, ...triage, ...grilling],
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
    [...triage, ...grilling],
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
    // item into the grilling queue, so the slot that reopens is the
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
    ? [...allItems, ...triage, ...grilling].find(
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
  //
  // Only ever taken on Now: `grill` is threaded by that caller alone, which
  // is also why `backLabel` below can name it outright.
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
          slot above the frontier is this board's own: ADR-0015 gives the
          *aside* to the ranked region, and its "standing questions never take
          the banner" is a claim about the aside's contents, not about this
          column. (On a project's board there is no such aside at all — it is
          Now's, not the board's.)

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
      frontier.length === 0 &&
      blocked.length === 0 &&
      triage.length === 0 &&
      grilling.length === 0 ? (
        <Card padding="var(--space-3)">
          <EmptyState
            icon="zap"
            headingLevel={2}
            title={emptyState.title}
            body={emptyState.body}
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
      {frontier.length > 0 || triage.length > 0 || grilling.length > 0 ? (
        <FrontierColumns
          frontier={frontier}
          triage={triage}
          grilling={grilling}
          draftItemIds={task.grillDraftItemIds}
          projects={task.projects ?? []}
          nowMs={nowMs}
          selectedItemId={selectedItemId}
          onOpenItem={onOpenItem}
          onAct={onAct}
          storage={storage}
          screen={screen}
          axes={axes}
        />
      ) : null}

      {blocked.length > 0 ? (
        <Section
          title="Blocked"
          meta={`${blocked.length} ${blocked.length === 1 ? "action" : "actions"}`}
        >
          <Card padding="var(--space-3)">
            {blocked.map((entry) => (
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
