import { useState } from "react";
import { Icon } from "../components/core/Icon";
import type { ProjectDTO, TaskItemDTO, TriageDestinationName } from "../store/protocol";
import type { TaskTriageResult } from "../store/store";
import type { TriageEdits } from "../store/worker-client";
import { TriageRow } from "./TriageRow";
import { orderTriage } from "./triage-order";
import {
  readTriageCollapsed,
  writeTriageCollapsed,
  type StorageLike,
} from "./triage-collapse";

/** Now's triage section: the same unsorted inbox the Triage screen renders,
 * under the frontier, with the same rows and the same full editor.
 *
 * Three placement decisions, all deliberate:
 *
 * **It lives under the promoted items, never above them.** Now answers "what
 * do I do next", and the frontier is that answer; sorting the inbox is the
 * other thing you do here, not the first thing you see.
 *
 * **It is rendered inside `RealFrontier`, not `NowScreen`'s `Column`.** The
 * item detail panel is a full-column takeover, and an editor left open
 * underneath it would be a second unsent draft on screen with nothing to say
 * which one is being worked — the same argument that keeps one triage row
 * open at a time.
 *
 * **It caps its own height and scrolls.** This is the first independent
 * scroll container in the centre column (`layout.tsx`'s `Aside` is the only
 * other one in the app, and its comment is the precedent for the reasoning):
 * without a cap, an inbox of thirty captures pushes the frontier off the top
 * of a page that already has its own scroll, and Now stops being about doing.
 * The cap is on the *list*, so the header stays put and the count is always
 * readable. Horizontal overflow is what the visual gate fails on, and this
 * container never produces any at the page level.
 *
 * Every triage decision belongs to somebody else: ordering is
 * `triage-order.ts`'s `orderTriage`, the row and its whole editor are
 * `TriageRow`, and the persisted collapse is `triage-collapse.ts`. */
export interface NowTriageSectionProps {
  /** `TaskState.triageInbox` — unordered; this component sorts it. */
  items: TaskItemDTO[];
  projects: ProjectDTO[];
  nowMs: number;
  /** `TaskState.lastTriage`, forwarded to every row so a failure lands on the
   * item it names (`TriageRow`'s own prop doc). */
  lastTriage?: TaskTriageResult | null;
  /** S13/#111's triage mutation. Absent in demo mode, in which case the rows
   * are readable and never expand into an editor that could send nothing. */
  onTriage?: (itemId: string, destination: TriageDestinationName, edits: TriageEdits) => void;
  /** The row checkmark's `Core::act` complete. Absent for the same reason. */
  onComplete?: (itemId: string) => void;
  /** Injected rather than read here, the same guard `RankedRegion` gets from
   * `NowScreen` — `undefined` in a context with no `localStorage`. */
  storage?: StorageLike;
}

export function NowTriageSection({
  items,
  projects,
  nowMs,
  lastTriage,
  onTriage,
  onComplete,
  storage,
}: NowTriageSectionProps) {
  // Expanded is the default (`triage-collapse.ts`): captures are in your face
  // until you drain them.
  const [collapsed, setCollapsed] = useState(() => readTriageCollapsed(storage));
  // One row open at a time — expanding is a *selection*, exactly as on the
  // Triage screen. Its selection and this one are deliberately independent:
  // they are different component instances, and a row opened here says
  // nothing about what the other screen should show.
  const [selectedId, setSelectedId] = useState<string | null>(null);

  // An empty inbox on Now is not a finding worth a card — good news is
  // reported by the absence of the section. The Triage screen keeps its own
  // `EmptyState`, which is where someone goes to ask "is it empty?".
  if (items.length === 0) {
    return null;
  }

  const ordered = orderTriage(items);
  const listId = "now-triage-list";

  function toggle(): void {
    const next = !collapsed;
    setCollapsed(next);
    writeTriageCollapsed(storage, next);
  }

  return (
    <div>
      {/* `h2` under the header's `h1`, matching `layout.tsx`'s `Section` —
          the button is inside the heading rather than around it, so the
          level survives and the control is still one tab stop. */}
      <h2 style={{ margin: 0, marginBottom: "var(--space-4)" }}>
        <button
          type="button"
          onClick={toggle}
          aria-expanded={!collapsed}
          aria-controls={collapsed ? undefined : listId}
          style={{
            display: "flex",
            alignItems: "baseline",
            justifyContent: "space-between",
            gap: "var(--space-4)",
            width: "100%",
            padding: 0,
            background: "transparent",
            border: "none",
            textAlign: "left",
            font: "inherit",
            color: "inherit",
            cursor: "pointer",
          }}
        >
          <span
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: "var(--space-3)",
              font: "var(--type-h3)",
              color: "var(--text-primary)",
            }}
          >
            {/* One chevron, rotated — the same idiom `TriageRow` uses for its
                own open/closed state, so the two nest without a second
                vocabulary for the same fact. */}
            <span
              style={{
                display: "inline-flex",
                transform: collapsed ? "none" : "rotate(180deg)",
                transition: "transform var(--dur-base) var(--ease-flit)",
              }}
            >
              <Icon name="chevron-down" size={16} color="var(--text-muted)" />
            </span>
            Triage
          </span>
          <span className="hb-meta">{ordered.length} unsorted</span>
        </button>
      </h2>

      {collapsed ? null : (
        <div
          id={listId}
          style={{
            display: "flex",
            flexDirection: "column",
            gap: "var(--space-4)",
            maxHeight: "60dvh",
            overflowY: "auto",
            // The cards inside carry a shadow at rest; without the padding
            // the scroll container clips it against its own edges.
            padding: "var(--space-2)",
            margin: "calc(var(--space-2) * -1)",
          }}
        >
          {ordered.map((item) => (
            <TriageRow
              key={item.id}
              item={item}
              projects={projects}
              expanded={selectedId === item.id}
              onToggle={() => setSelectedId(selectedId === item.id ? null : item.id)}
              nowMs={nowMs}
              onTriage={onTriage}
              onComplete={onComplete}
              lastTriage={lastTriage}
            />
          ))}
        </div>
      )}
    </div>
  );
}
