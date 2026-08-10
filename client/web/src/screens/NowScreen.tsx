import { useState } from "react";
import { Badge } from "../components/core/Badge";
import { Button } from "../components/core/Button";
import { Card } from "../components/core/Card";
import { Icon } from "../components/core/Icon";
import { ContextTile } from "../components/domain/ContextTile";
import { ItemDetailPanel } from "../components/domain/ItemDetailPanel";
import { ItemRow } from "../components/domain/ItemRow";
import { StageBadge } from "../components/domain/StageBadge";
import { EmptyState } from "../components/feedback/EmptyState";
import type { CalendarTileProps } from "../calendar/tile-props";
import type { DemoData, DemoSnapshot } from "../fixtures/demo";
import type { Screen } from "../shell/screens";
import type { TaskActionName, TaskItemDTO } from "../store/protocol";
import type { TaskState } from "../store/store";
import { blockedReasonLabel } from "./blocked-reason";
import { groupByProject } from "./frontier-groups";
import { orderFrontier } from "./frontier-order";
import { applyItemAction } from "./item-actions";
import { Aside, Column, Section, TwoColumn } from "./layout";
import { computeUrgency } from "./urgency";

function SnapshotTile({ snapshot }: { snapshot: DemoSnapshot }) {
  return (
    <Card
      padding="var(--space-5)"
      style={{ flex: 1, display: "flex", flexDirection: "column", gap: "var(--space-2)" }}
    >
      <span className="hb-meta">{snapshot.name}</span>
      <span style={{ font: "var(--weight-bold) 22px/1.1 var(--font-display)", color: "var(--text-primary)" }}>
        {snapshot.value}
      </span>
      <span className="hb-meta" style={{ color: "var(--text-muted)" }}>
        {snapshot.note}
      </span>
    </Card>
  );
}

export interface NowScreenProps {
  demo: DemoData | null;
  /** The real calendar context, or null on a device that never opted in. */
  tile: CalendarTileProps | null;
  onScreen: (screen: Screen) => void;
  /** S10's real frontier data (issue #108) — rendered whenever `demo` is
   * null, i.e. always outside dev's `?demo` mode. */
  task: TaskState;
  /** Sampled at the same ~30s granularity `useCalendarWiring.ts` already
   * ticks at — coarse enough for urgency's own bucket sizes
   * (`urgency.ts`), so this reuses that clock rather than adding a second
   * one. */
  nowMs: number;
  selectedItemId: string | null;
  onOpenItem: (itemId: string) => void;
  onCloseItemDetail: () => void;
  /** S11/#109's act affordances (start/complete/block/cancel), forwarded to
   * whichever item is currently open in detail. */
  onAct: (itemId: string, action: TaskActionName) => void;
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
  // block succeeds even though the panel — and its now-reachable "Start"/
  // "Cancel" row (`availableActions("blocked")`) — should stay showing.
  // `optimisticItem` is the fallback: `applyItemAction` mirrors the same
  // action->stage mapping `Core::act` itself applies, so the panel shows
  // the real post-action state immediately rather than either freezing on
  // stale pre-action data or going blank. Cleared whenever `selectedItemId`
  // itself changes (a different item opened, or the panel closed) so a
  // stale optimistic item from a PREVIOUS selection can never leak into a
  // new one.
  const [optimisticItem, setOptimisticItem] = useState<TaskItemDTO | null>(null);
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
  }

  const selectedItem =
    liveSelectedItem ??
    (optimisticItem && optimisticItem.id === selectedItemId ? optimisticItem : null);

  if (selectedItem) {
    return (
      <ItemDetailPanel
        item={selectedItem}
        steps={task.stepsByItem[selectedItem.id] ?? []}
        onClose={onCloseItemDetail}
        onAct={(action) => {
          setOptimisticItem(applyItemAction(selectedItem, action));
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
  tile,
  onScreen,
  task,
  nowMs,
  selectedItemId,
  onOpenItem,
  onCloseItemDetail,
  onAct,
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
        <div>
          <span className="hb-meta">calendar context</span>
          <div
            style={{
              marginTop: "var(--space-4)",
              display: "flex",
              flexDirection: "column",
              gap: "var(--space-4)",
            }}
          >
            {demo ? (
              <>
                <ContextTile
                  kind="in_progress"
                  title="Design review"
                  timeLabel="9:30–10:00 AM"
                  href="#"
                  asOf="just now"
                />
                <ContextTile
                  kind="upcoming"
                  title="School pickup"
                  timeLabel="3:10–3:30 PM"
                  asOf="42m ago"
                  stale
                />
              </>
            ) : tile ? (
              <ContextTile
                kind={tile.kind}
                title={tile.title}
                timeLabel={tile.timeLabel}
                href={tile.href}
                asOf={tile.asOf}
                stale={tile.stale}
              />
            ) : (
              <Card padding="var(--space-3)">
                <EmptyState
                  compact
                  icon="calendar-clock"
                  headingLevel={2}
                  title="No calendar connected"
                  body="Connect Google Calendar in Settings to see what's on now and next."
                />
              </Card>
            )}
          </div>
        </div>

        {demo ? (
          <>
            <div>
              <span className="hb-meta">standing questions</span>
              <Card
                padding="var(--space-5)"
                style={{
                  marginTop: "var(--space-4)",
                  display: "flex",
                  flexDirection: "column",
                  gap: "var(--space-4)",
                }}
              >
                {demo.standingQuestions.map((question) => (
                  <div key={question.q} style={{ display: "flex", gap: "var(--space-4)" }}>
                    <Icon
                      name="help-circle"
                      size={16}
                      color="var(--text-muted)"
                      style={{ marginTop: 2 }}
                    />
                    <div>
                      <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
                        {question.q}
                      </p>
                      <p style={{ font: "var(--type-body-strong)" }}>{question.a}</p>
                    </div>
                  </div>
                ))}
              </Card>
            </div>
            <div>
              <span className="hb-meta">context snapshots</span>
              <div
                style={{
                  marginTop: "var(--space-4)",
                  display: "flex",
                  gap: "var(--space-4)",
                  flexWrap: "wrap",
                }}
              >
                {demo.snapshots.map((snapshot) => (
                  <SnapshotTile key={snapshot.name} snapshot={snapshot} />
                ))}
              </div>
            </div>
          </>
        ) : null}
      </Aside>
    </TwoColumn>
  );
}
