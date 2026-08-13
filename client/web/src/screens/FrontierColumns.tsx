// Now's centre column: the frontier as wrapping columns (#402, ADR-0021).
// Everything decidable lives in `frontier-columns.ts`; this file threads state
// through it and paints the result — the split every `screens/*` module keeps.
//
// The card is local rather than a sixteenth entry in `components/domain/`: it
// is used by this surface and no other, and `ItemRow` — which four screens
// share — stays exactly as it is for Triage, Done, Ledger and Now's own
// Blocked section. Two components because they have genuinely different
// densities and affordances, not a variant flag on one.

import { useState } from "react";
import { Badge } from "../components/core/Badge";
import { Card } from "../components/core/Card";
import { MarkDoneButton } from "../components/domain/MarkDoneButton";
import { StageBadge } from "../components/domain/StageBadge";
import {
  DEFAULT_FRONTIER_AXIS,
  FRONTIER_AXES,
  groupFrontier,
  type FrontierAxis,
} from "./frontier-columns";
import { orderFrontier } from "./frontier-order";
import { canMarkDone } from "./item-actions";
import { hasPriority, priorityLabel } from "./priority";
import { computeUrgency, type Urgency } from "./urgency";
import type { ProjectDTO, TaskItemDTO } from "../store/protocol";

const AXIS_LABEL: Record<FrontierAxis, string> = {
  context: "Context",
  project: "Project",
  size: "Size",
  energy: "Energy",
};

/** Display text for the column of items naming no value on the live axis —
 * `frontier-columns.ts` returns `value: null` and leaves the words here. */
const NO_VALUE_LABEL: Record<FrontierAxis, string> = {
  context: "No context",
  project: "No project",
  size: "No size",
  energy: "No energy",
};

/** ADR-0021 decision 2: **colour encodes urgency and nothing else.** `calm` is
 * absent from the legend on purpose — the default is not a claim worth
 * colouring, so it takes the same hairline every card already has. */
const LEGEND: readonly Urgency[] = ["overdue", "now", "soon"];

const URGENCY_EDGE: Record<Urgency, string> = {
  overdue: "var(--urgency-overdue)",
  now: "var(--urgency-now)",
  soon: "var(--urgency-soon)",
  calm: "var(--border-subtle)",
};

const URGENCY_LABEL: Record<Urgency, string> = {
  overdue: "Overdue",
  now: "Due now",
  soon: "Due soon",
  calm: "Calm",
};

/** Cards shown per column before the `n more` toggle. The cap is what makes
 * wrapping work — a wrapping row takes its height from the tallest column in
 * its line, so one fat column would otherwise strand its neighbours in
 * whitespace — and it is the honest cap for this surface anyway: the top few
 * of a column is what "what's next" is asking about. */
const COLUMN_CAP = 6;

function ItemCard({
  item,
  nowMs,
  selected,
  onOpen,
  onComplete,
}: {
  item: TaskItemDTO;
  nowMs: number;
  selected: boolean;
  onOpen: () => void;
  onComplete?: () => void;
}) {
  const urgency = computeUrgency(item.deadline, nowMs);
  return (
    // `role="button"` on a container rather than `as="button"`, for the reason
    // `ItemRow` does the same: the mark-done checkmark is itself a button, and
    // a button inside a button is invalid. Enter/Space are wired by hand, and
    // the `event.target === event.currentTarget` guard keeps a keypress on the
    // checkmark from also opening the card.
    <Card
      role="button"
      tabIndex={0}
      interactive
      elevation={0}
      padding="var(--space-4)"
      accent={selected}
      onClick={onOpen}
      onKeyDown={(event) => {
        if (event.target === event.currentTarget && (event.key === "Enter" || event.key === " ")) {
          event.preventDefault();
          event.currentTarget.click();
        }
      }}
      aria-current={selected ? "true" : undefined}
      style={{
        display: "flex",
        gap: "var(--space-4)",
        alignItems: "stretch",
        textAlign: "left",
        // The card stays marked while its item is the one open — the reader has
        // to be able to see where the thing they picked came from, and what it
        // was sitting next to.
        background: selected ? "var(--accent-quiet)" : "var(--surface-card)",
      }}
    >
      {/* Urgency, as the card's leading edge. Deliberately an element inside
          the card and NOT `borderLeft` on the card itself: the design system's
          card rule is "never a coloured left border", and the prototype broke
          it. An inset bar satisfies both — the card keeps its hairline and its
          radius, and the edge still reads as the leading edge. */}
      <span
        aria-hidden="true"
        style={{
          width: 3,
          flex: "0 0 auto",
          borderRadius: 2,
          background: URGENCY_EDGE[urgency],
        }}
      />
      <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
        <span style={{ font: "var(--type-body)", color: "var(--text-primary)", lineHeight: 1.35 }}>
          {item.title}
        </span>
        <span
          style={{
            display: "flex",
            alignItems: "center",
            gap: "var(--space-3)",
            flexWrap: "wrap",
          }}
        >
          {/* Colour carries urgency, so the card says it in words too. Text
              rather than `ItemRow`'s `title` tooltip, which a keyboard or
              screen-reader user does not reliably get (ADR-0021 decision 2). */}
          <span
            className="hb-meta"
            style={{
              color: urgency === "calm" ? "var(--text-muted)" : URGENCY_EDGE[urgency],
            }}
          >
            {URGENCY_LABEL[urgency]}
          </span>
          {/* "Ready" is the default and says nothing at card size — the stage
              chip earns its width only once the item is already running. */}
          {item.stage === "ready" ? null : <StageBadge stage={item.stage} />}
          {item.size ? <Badge mono>{item.size}</Badge> : null}
          {hasPriority(item.priority) ? (
            <span className="hb-meta" style={{ color: "var(--text-brand)" }}>
              {priorityLabel(item.priority)}
            </span>
          ) : null}
          {item.deadline ? <span className="hb-meta">{item.deadline}</span> : null}
          {item.scheduledDate ? (
            <span className="hb-meta" style={{ color: "var(--text-muted)" }}>
              {item.scheduledDate}
            </span>
          ) : null}
          {item.pending ? (
            <span className="hb-meta" style={{ color: "var(--text-muted)" }}>
              Pending
            </span>
          ) : null}
        </span>
      </span>
      {onComplete ? (
        <MarkDoneButton
          title={item.title}
          disabled={item.pending}
          onClick={(event) => {
            // Finishing something must never also open it.
            event.stopPropagation();
            onComplete();
          }}
        />
      ) : null}
    </Card>
  );
}

function controlStyle(selected: boolean) {
  return {
    font: "var(--type-body-sm)",
    minHeight: "var(--row-height)",
    padding: "var(--space-3) var(--space-5)",
    borderRadius: "var(--radius-control)",
    border: `1px solid ${selected ? "var(--accent-quiet-border)" : "var(--border-default)"}`,
    background: selected ? "var(--accent-quiet)" : "transparent",
    color: selected ? "var(--text-brand)" : "var(--text-secondary)",
    cursor: "pointer",
    display: "flex",
    alignItems: "center",
    gap: "var(--space-2)",
  };
}

export function FrontierColumns({
  frontier,
  projects,
  nowMs,
  selectedItemId,
  onOpenItem,
  onAct,
}: {
  frontier: readonly TaskItemDTO[];
  projects: readonly ProjectDTO[];
  nowMs: number;
  selectedItemId: string | null;
  onOpenItem: (itemId: string) => void;
  onAct: (itemId: string, action: "complete") => void;
}) {
  // Local for this slice. #403 persists it device-locally through the
  // `storage` seam `NowScreen` already threads, and clears the per-column
  // state with it.
  const [axis, setAxis] = useState<FrontierAxis>(DEFAULT_FRONTIER_AXIS);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set<string>());

  // `orderFrontier` unchanged, applied once before grouping — the grouping
  // module preserves input order inside each column, so the within-column
  // rule is `orderFrontier` and there is no second ordering function.
  const columns = groupFrontier(orderFrontier([...frontier]), axis, projects);

  const pickAxis = (next: FrontierAxis) => {
    setAxis(next);
    // The `n more` expansions are keyed by column, and switching the axis
    // means those columns no longer exist.
    setExpanded(new Set<string>());
  };

  const toggleExpanded = (key: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  return (
    <>
      <div
        style={{
          display: "flex",
          gap: "var(--space-2)",
          alignItems: "center",
          flexWrap: "wrap",
        }}
      >
        <span className="hb-meta" style={{ marginRight: "var(--space-3)" }}>
          group by
        </span>
        {FRONTIER_AXES.map((entry) => (
          <button
            key={entry}
            type="button"
            aria-pressed={axis === entry}
            onClick={() => pickAxis(entry)}
            style={controlStyle(axis === entry)}
          >
            {AXIS_LABEL[entry]}
          </button>
        ))}
      </div>

      {/* The one thing colour says here, stated once as a key. */}
      <div style={{ display: "flex", gap: "var(--space-5)", flexWrap: "wrap", alignItems: "center" }}>
        {LEGEND.map((urgency) => (
          <span key={urgency} style={{ display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
            <span
              aria-hidden="true"
              style={{ width: 3, height: 12, borderRadius: 2, background: URGENCY_EDGE[urgency] }}
            />
            <span className="hb-meta">{URGENCY_LABEL[urgency]}</span>
          </span>
        ))}
      </div>

      {/* Wrapping columns, never a sideways-scrolling strip: they flow onto as
          many lines as the width needs, in reading order, and this container
          adds no scroll of its own — `docs/SURFACES.md` records the triage
          cap as the only independent scroll container in the centre column,
          and that stays true (ADR-0021 decision 3). */}
      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          gap: "var(--space-6)",
          alignItems: "flex-start",
        }}
      >
        {columns.map((column) => {
          const key = column.value ?? "";
          const heading =
            column.value === null
              ? NO_VALUE_LABEL[axis]
              : (column.label ?? `Project ${column.value}`);
          const isOpen = expanded.has(key);
          const visible = isOpen ? column.items : column.items.slice(0, COLUMN_CAP);
          const hidden = column.items.length - visible.length;
          return (
            <div
              key={key}
              style={{
                flex: "1 1 240px",
                minWidth: 240,
                // Wide enough that a narrow window — where only one column
                // fits beside the aside — fills its width instead of
                // stranding a strip of empty page.
                maxWidth: 380,
                display: "flex",
                flexDirection: "column",
                gap: "var(--space-3)",
              }}
            >
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "var(--space-3)",
                  padding: "var(--space-2)",
                }}
              >
                <h2
                  style={{
                    flex: 1,
                    minWidth: 0,
                    margin: 0,
                    font: "var(--type-body-strong)",
                    color: "var(--text-primary)",
                  }}
                >
                  {heading}
                </h2>
                <span className="hb-meta">{column.items.length}</span>
              </div>
              {visible.map((item) => (
                <ItemCard
                  key={item.id}
                  item={item}
                  nowMs={nowMs}
                  selected={item.id === selectedItemId}
                  onOpen={() => onOpenItem(item.id)}
                  onComplete={canMarkDone(item) ? () => onAct(item.id, "complete") : undefined}
                />
              ))}
              {/* The count never lies about what is hidden. */}
              {hidden > 0 || isOpen ? (
                <button
                  type="button"
                  onClick={() => toggleExpanded(key)}
                  style={{
                    font: "var(--type-body-sm)",
                    minHeight: "var(--row-height)",
                    background: "none",
                    border: "none",
                    color: "var(--text-link)",
                    cursor: "pointer",
                    textAlign: "left",
                    padding: "0 var(--space-2)",
                  }}
                >
                  {isOpen ? "Show fewer" : `${hidden} more`}
                </button>
              ) : null}
            </div>
          );
        })}
      </div>
    </>
  );
}
