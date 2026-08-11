import { useState } from "react";
import { Badge } from "../components/core/Badge";
import { Button } from "../components/core/Button";
import { Card } from "../components/core/Card";
import { ItemDetailPanel } from "../components/domain/ItemDetailPanel";
import { ItemRow } from "../components/domain/ItemRow";
import { StageBadge } from "../components/domain/StageBadge";
import { EmptyState } from "../components/feedback/EmptyState";
import type { DemoData } from "../fixtures/demo";
import { demoQuestionInputs } from "../fixtures/demo-questions";
import type { Screen } from "../shell/screens";
import type { CalendarReadDTO, TaskActionName, TaskItemDTO } from "../store/protocol";
import type { TaskState } from "../store/store";
import { blockedReasonLabel } from "./blocked-reason";
import { groupByProject } from "./frontier-groups";
import { orderFrontier } from "./frontier-order";
import { applyItemAction, resolveFallbackPending } from "./item-actions";
import { Aside, Column, Section, TwoColumn } from "./layout";
import type { QuestionInputs } from "./questions/contract";
import { RankedRegion } from "./questions/RankedRegion";
// PROTOTYPE (#119) — throwaway, dev-only, renders nothing without `?racepane`.
// Shape settled (context panel); delete these two mounts with
// `prototype-race-pane/`.
import { RacePane, RacePaneSwitcher } from "./prototype-race-pane/RacePanePrototype";
// PROTOTYPE (#121) — throwaway, dev-only, renders nothing without
// `?vacationpane`. Shape decided (A, context panel); delete these two mounts
// with `prototype-vacation-pane/` once it is folded into the real pane.
import {
  VacationPane,
  VacationPaneSwitcher,
} from "./prototype-vacation-pane/VacationPanePrototype";
import { computeUrgency } from "./urgency";

export interface NowScreenProps {
  demo: DemoData | null;
  onScreen: (screen: Screen) => void;
  /** S10's real frontier data (issue #108) — rendered whenever `demo` is
   * null, i.e. always outside dev's `?demo` mode. */
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
  /** #122's do-date write affordance — forwarded straight to `RankedRegion`,
   * `undefined` in demo mode like every other real-write callback here. */
  onSetScheduledDate?: (itemId: string, date: string | null) => void;
}

/** The real-data half of `QuestionInputs` (never demo's) — exported so a
 * component test can drive the exact object this screen threads to
 * `RankedRegion` through a genuinely mounted consumer, proving delivery
 * rather than merely inspecting the store snapshot (#267's review point).
 *
 * `items` (#122) is `task.frontier` and `task.blocked`'s items unioned —
 * exactly `RealFrontier`'s own `allItems`, computed a second time here
 * because that helper is scoped to `RealFrontier`'s render and this
 * function has to stay callable standalone by both `NowScreen` and its own
 * tests. */
export function realQuestionInputs(
  task: TaskState,
  calendarReads: Record<string, CalendarReadDTO | undefined>,
  calendarConnected: boolean,
): Omit<QuestionInputs, "nowMs"> {
  return {
    bindings: task.bindings,
    paneReads: task.paneReads,
    calendarReads,
    calendarConnected,
    items: [...task.frontier, ...task.blocked.map((entry) => entry.item)],
  };
}

/** Real-data frontier/blocked rendering (issue #108) — kept out of the
 * `demo`-fixture render path above so the two never entangle: the fixture
 * carries its own (deliberately hand-authored) `urgency`/`blockedBy`
 * strings, while this branch derives everything at read time from the
 * `TaskItemDTO`s the store actually holds. */
function RealFrontier({
  task,
  nowMs,
  selectedItemId,
  onOpenItem,
  onCloseItemDetail,
  onAct,
}: Pick<
  NowScreenProps,
  "task" | "nowMs" | "selectedItemId" | "onOpenItem" | "onCloseItemDetail" | "onAct"
>) {
  // Reviewer finding on PR #207: a failed `actResult` used to be recorded
  // in `TaskState.lastAct` and rendered nowhere — this is what makes it
  // visible, matched to the currently open item by id so a stale failure
  // from a DIFFERENT item never bleeds into this one.
  const actError =
    task.lastAct && task.lastAct.itemId === selectedItemId && task.lastAct.kind !== "ok"
      ? (task.lastAct.error ?? "That action didn't apply.")
      : null;

  const allItems = [...task.frontier, ...task.blocked.map((entry) => entry.item)];
  const liveSelectedItem = selectedItemId
    ? (allItems.find((item) => item.id === selectedItemId) ?? null)
    : null;

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

  if (selectedItem) {
    return (
      <ItemDetailPanel
        item={selectedItem}
        steps={task.stepsByItem[selectedItem.id] ?? []}
        onClose={onCloseItemDetail}
        onAct={(action) => {
          setOptimisticItem(applyItemAction(selectedItem, action));
          setAwaitingPendingConfirm(true);
          onAct(selectedItem.id, action);
        }}
        actError={actError}
      />
    );
  }

  const groups = groupByProject(orderFrontier(task.frontier), task.projects);

  if (groups.length === 0 && task.blocked.length === 0) {
    return (
      <Card padding="var(--space-3)">
        <EmptyState
          icon="zap"
          headingLevel={2}
          title="Nothing to start"
          body="No actions are Ready or In Progress right now."
        />
      </Card>
    );
  }

  return (
    <>
      {groups.map((group) => (
        <Section
          key={group.projectId ?? "unassigned"}
          title={
            group.projectId === null
              ? "No project"
              : (group.projectName ?? `Project ${group.projectId}`)
          }
          meta={`${group.items.length} ${group.items.length === 1 ? "action" : "actions"}`}
        >
          <Card padding="var(--space-3)">
            {group.items.map((item) => (
              <ItemRow
                key={item.id}
                title={item.title}
                stage={item.stage}
                urgency={computeUrgency(item.deadline, nowMs)}
                deadline={item.deadline ?? undefined}
                scheduled={item.scheduledDate ?? undefined}
                size={item.size ?? undefined}
                priority={item.priority}
                pending={item.pending}
                onClick={() => onOpenItem(item.id)}
              />
            ))}
          </Card>
        </Section>
      ))}

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
                  size={entry.item.size ?? undefined}
                  priority={entry.item.priority}
                  pending={entry.item.pending}
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
  demo,
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
}: NowScreenProps) {
  // Ranking is not implemented, so the hero picks by the one property that
  // makes an item obviously the current one — not by fixture position, which
  // would let a reordered fixture describe the wrong action.
  const top = demo
    ? (demo.items.find((item) => item.stage === "in_progress") ?? demo.items[0])
    : undefined;
  const rest = demo ? demo.items.filter((item) => item.id !== top?.id && item.stage !== "done") : [];

  return (
    <TwoColumn>
      <RacePaneSwitcher />
      <VacationPaneSwitcher />
      <Column>
        {demo && top ? (
          <>
            <div>
              <span className="hb-meta">top pick</span>
              <Card
                accent
                elevation={2}
                padding="var(--space-7)"
                style={{
                  marginTop: "var(--space-4)",
                  display: "flex",
                  flexDirection: "column",
                  gap: "var(--space-5)",
                }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: "var(--space-4)" }}>
                  <StageBadge stage={top.stage} />
                  {top.size ? <Badge mono>size:{top.size}</Badge> : null}
                  <Badge mono tone="brand">
                    {top.id}
                  </Badge>
                </div>
                <h2 style={{ font: "var(--type-h1)", letterSpacing: "var(--tracking-heading)" }}>
                  {top.title}
                </h2>
                {top.note ? (
                  <p style={{ font: "var(--type-body)", color: "var(--text-secondary)" }}>
                    {top.note}
                  </p>
                ) : null}
                <div style={{ display: "flex", gap: "var(--space-4)", flexWrap: "wrap" }}>
                  <Button iconLeft="play">Resume</Button>
                  <Button variant="secondary" iconLeft="list-checks" onClick={() => onScreen("routes")}>
                    Steps ({top.steps})
                  </Button>
                  <Button variant="ghost" iconLeft="clock">
                    Later today
                  </Button>
                </div>
              </Card>
            </div>
            <Section
              title="Also startable"
              meta={`${rest.length} ${rest.length === 1 ? "action" : "actions"}`}
            >
              <Card padding="var(--space-3)">
                {rest.map((item) => (
                  <ItemRow
                    key={item.id}
                    title={item.title}
                    stage={item.stage}
                    urgency={item.urgency}
                    deadline={item.deadline}
                    scheduled={item.scheduled}
                    size={item.size}
                    steps={item.steps}
                    blockedBy={item.blockedBy}
                    onClick={() => onScreen("routes")}
                  />
                ))}
              </Card>
            </Section>
          </>
        ) : (
          <RealFrontier
            task={task}
            nowMs={nowMs}
            selectedItemId={selectedItemId}
            onOpenItem={onOpenItem}
            onCloseItemDetail={onCloseItemDetail}
            onAct={onAct}
          />
        )}
      </Column>

      <Aside label="Context">
        {/* ADR-0015's ranked region replaces everything that used to be in
            here — the context tile, the demo standing-question card and the
            snapshot tiles — and it is the same component in both modes: only
            the inputs differ, so `?demo` photographs the real shell. */}
        <RankedRegion
          inputs={
            demo
              ? demoQuestionInputs(nowMs)
              : realQuestionInputs(task, calendarReads, calendarConnected)
          }
          nowMs={nowMs}
          syncOutcomeSeq={task.syncOutcomeSeq}
          storage={typeof localStorage === "undefined" ? undefined : localStorage}
          onScreen={onScreen}
          onSetScheduledDate={demo ? undefined : onSetScheduledDate}
        />
        {/* The surviving prototypes (#119/#121), still dev-only and
            param-gated, sit beside the region until each is folded into a
            real question of its own — #122's own weekend pane is folded in
            already, registered in the shell above. */}
        <RacePane />
        <VacationPane />
      </Aside>
    </TwoColumn>
  );
}
