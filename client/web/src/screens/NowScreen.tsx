// Now: the shell's default screen — composition and nothing else. Its centre
// column is `FrontierBoard.tsx` (shared with the project dossier since the
// dossier became the same board filtered to one project), and its aside is
// ADR-0015's ranked region. What still lives here is what is genuinely Now's:
// the standing-questions aside beside the board, `realQuestionInputs` (which
// the Status screen shares), and the `grill` wiring the board only ever gets
// on this surface.

import type { GrillTakeoverWiring } from "../shell/useGrillTakeoverWiring";
import type { Screen } from "../shell/screens";
import type { MicrotaskWiring } from "../shell/useMicrotaskWiring";
import type { CalendarReadDTO, TaskActionName } from "../store/protocol";
import type { TaskState } from "../store/store";
import type { TriageEdits } from "../store/worker-client";
import { FrontierBoard } from "./FrontierBoard";
import { Aside, Column, TwoColumn } from "./layout";
import type { QuestionInputs } from "./questions/contract";
import { RankedRegion } from "./questions/RankedRegion";
import type { StorageLike } from "./storage";

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
 * `items` (#122) is `task.frontier` and `task.blocked`'s items unioned —
 * exactly `FrontierBoard`'s own `allItems`, computed a second time here
 * because that helper is scoped to that component's render and this
 * function has to stay callable standalone by both `NowScreen` and its own
 * tests. */
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
    items: [...task.frontier, ...task.blocked.map((entry) => entry.item)],
  };
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
        {/* The board itself is `FrontierBoard.tsx`, shared with the project
            dossier's centre column. Now passes the store's own four queries
            unfiltered — that is the only difference between this call and the
            project board's. */}
        <FrontierBoard
          task={task}
          frontier={task.frontier}
          triage={task.triageInbox}
          grilling={task.grillingItems}
          blocked={task.blocked}
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
          screen="now"
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
