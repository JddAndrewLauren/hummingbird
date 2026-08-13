// PROTOTYPE — throwaway. Delete with the rest of `now-prototype/`.
//
// Variant C — the frontier in **columns**, grouped by a switchable axis, plus
// the grafts and decisions of four later rounds:
//
//   * A's facet chips, behind a Filter button rather than always on screen —
//     the axis switch is the everyday control and filtering is the occasional
//     one, so only one of them earns permanent space.
//   * **Colour encodes urgency and nothing else** (round 5). B's six mixed
//     buckets are gone: colour reads *across* whichever axis the columns are
//     grouped by, so it must say one thing, and CONTEXT.md's `Urgency` is the
//     one thing it already means everywhere else in this app. Size and
//     scheduled date are on the card as their own chips.
//   * Selecting a card expands the real `ItemDetailPanel` above the columns
//     rather than replacing them (round 4) — see `index.tsx`.
//
// And no horizontal scrolling: the columns wrap onto as many lines as the
// width needs, each capped at `COLUMN_CAP` cards, each collapsible in place.
// A strip you scroll sideways hides columns; this only ever gets taller.

import { useState } from "react";
import { Badge } from "../../components/core/Badge";
import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { StageBadge } from "../../components/domain/StageBadge";
import { EmptyState } from "../../components/feedback/EmptyState";
import type { TaskItemDTO } from "../../store/protocol";
import type { VariantProps } from "./contract";
import { FacetRow } from "./facet-chips";
import {
  byAttention,
  contextsOf,
  ENERGIES,
  facetCount,
  matchesFacets,
  NO_CONTEXT,
  NO_FACETS,
  SIZES,
  toggleFacet,
  urgencyOf,
  type FacetSelection,
} from "./facets";
import type { Urgency } from "../urgency";
import { hasPriority, priorityLabel } from "../priority";

type Axis = "context" | "project" | "size" | "energy";

const AXES: { key: Axis; label: string }[] = [
  { key: "context", label: "Context" },
  { key: "project", label: "Project" },
  { key: "size", label: "Size" },
  { key: "energy", label: "Energy" },
];

/** Round 5's decision: **colour encodes urgency and nothing else.** The six
 * mixed buckets B invented (world pressure + your own do-date + the shape of
 * the work) are gone from this variant — CONTEXT.md's `Urgency` is a real term
 * with real tokens, the design system's "colour always encodes stage, tier or
 * urgency" rule is then satisfied with no exception to argue, and size and
 * scheduled date are already legible on the card as their own chips. `calm`
 * gets no swatch: no claim is not a claim worth colouring. */
const LEGEND: Urgency[] = ["overdue", "now", "soon"];

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

/** Cards shown per column before the "n more" toggle. */
const COLUMN_CAP = 6;

function ItemCard({
  item,
  nowMs,
  selected,
  onOpen,
}: {
  item: TaskItemDTO;
  nowMs: number;
  /** True while this card's item is the one expanded above the board. */
  selected: boolean;
  onOpen: () => void;
}) {
  const urgency = urgencyOf(item, nowMs);
  return (
    <Card
      as="button"
      interactive
      elevation={0}
      padding="var(--space-4)"
      onClick={onOpen}
      aria-current={selected ? "true" : undefined}
      // The edge carries urgency in colour alone, so it says it in words too —
      // `ItemRow`'s own dot does exactly this for the same reason.
      title={URGENCY_LABEL[urgency]}
      style={{
        width: "100%",
        display: "flex",
        flexDirection: "column",
        alignItems: "stretch",
        gap: "var(--space-3)",
        textAlign: "left",
        // The expanded card stays marked in the board — the reader has to be
        // able to see where the thing at the top came from, and what it was
        // sitting next to.
        background: selected ? "var(--accent-quiet)" : "var(--surface-card)",
        // Urgency, as the card's leading edge — the one thing colour says on
        // this surface. It replaces the urgency dot the first pass had here:
        // two encodings of the same fact on one 240px card is one too many.
        borderLeft: `3px solid ${URGENCY_EDGE[urgency]}`,
      }}
    >
      <span
        style={{
          font: "var(--type-body)",
          color: "var(--text-primary)",
          lineHeight: 1.35,
        }}
      >
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
        {/* "Ready" is the default and says nothing at card size — the stage
            chip earns its width only when the item is already running. */}
        {item.stage === "ready" ? null : <StageBadge stage={item.stage} />}
        {item.size ? <Badge mono>{item.size}</Badge> : null}
        {hasPriority(item.priority) ? (
          <span className="hb-meta" style={{ color: "var(--text-brand)" }}>
            {priorityLabel(item.priority)}
          </span>
        ) : null}
        {item.deadline ? (
          <span
            className="hb-meta"
            style={{
              color:
                urgency === "overdue" || urgency === "now"
                  ? URGENCY_EDGE[urgency]
                  : "var(--text-muted)",
            }}
          >
            {item.deadline.slice(5)}
          </span>
        ) : null}
      </span>
    </Card>
  );
}

export function VariantC({ items, projects, nowMs, selectedId, onOpenItem }: VariantProps) {
  const [axis, setAxis] = useState<Axis>("context");
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [picked, setPicked] = useState<FacetSelection>(NO_FACETS);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set<string>());
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set<string>());

  function toggleIn(
    set: (update: (current: ReadonlySet<string>) => ReadonlySet<string>) => void,
    key: string,
  ) {
    set((current) => {
      const next = new Set(current);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  }

  // Both per-column states are keyed by the column's own label, so switching
  // the axis re-labels every column and the old keys would apply to whatever
  // happened to share a name. Cleared with the switch instead.
  const pickAxis = (next: Axis) => {
    setAxis(next);
    setExpanded(new Set<string>());
    setCollapsed(new Set<string>());
  };

  const namesById = new Map(projects.map((project) => [project.id, project.name]));
  const active = facetCount(picked);

  const keyOf = (item: TaskItemDTO): string => {
    if (axis === "context") {
      return item.context ?? NO_CONTEXT;
    }
    if (axis === "project") {
      return item.projectId === null
        ? "no project"
        : (namesById.get(item.projectId) ?? "unknown project");
    }
    if (axis === "size") {
      return item.size ?? "no size";
    }
    return item.energy ?? "no energy";
  };

  const shown = [...items]
    .filter((item) => matchesFacets(item, picked, nowMs))
    .sort(byAttention(nowMs));

  const columns = new Map<string, TaskItemDTO[]>();
  for (const item of shown) {
    const key = keyOf(item);
    const bucket = columns.get(key);
    if (bucket) {
      bucket.push(item);
    } else {
      columns.set(key, [item]);
    }
  }
  // Fullest column first — the board's own answer to "where is the work".
  const ordered = [...columns.entries()].sort((a, b) => b[1].length - a[1].length);
  // A collapsed column keeps its place in the board — no separate strip. What
  // makes that work is the shrink-to-fit width below: a wrapping row takes its
  // height from the tallest column in the line, so a collapsed column that
  // still claimed a full-width slot would leave a hole rather than buy space.

  const segment = (selected: boolean) => ({
    font: "var(--type-body-sm)",
    padding: "var(--space-3) var(--space-5)",
    borderRadius: "var(--radius-control)",
    border: `1px solid ${selected ? "var(--accent-quiet-border)" : "var(--border-default)"}`,
    background: selected ? "var(--accent-quiet)" : "transparent",
    color: selected ? "var(--text-brand)" : "var(--text-secondary)",
    cursor: "pointer",
    display: "flex",
    alignItems: "center",
    gap: "var(--space-2)",
  });

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
        {AXES.map((entry) => (
          <button
            key={entry.key}
            type="button"
            aria-pressed={axis === entry.key}
            onClick={() => pickAxis(entry.key)}
            style={segment(axis === entry.key)}
          >
            {entry.label}
          </button>
        ))}

        {/* Filtering is the occasional gesture, so it costs one button rather
            than four permanent chip rows — but the button carries its own
            count, because a filtered board that looks unfiltered is a lie. */}
        <button
          type="button"
          aria-expanded={filtersOpen}
          onClick={() => setFiltersOpen(!filtersOpen)}
          style={{ ...segment(active > 0), marginLeft: "var(--space-4)" }}
        >
          <Icon name="search" size={14} />
          Filter
          {active > 0 ? <Badge mono tone="brand">{active}</Badge> : null}
        </button>
        {/* Only when the panel is shut: open, the panel states the same count
            at the foot of the chips it belongs to. */}
        {active > 0 && !filtersOpen ? (
          <span className="hb-meta" style={{ marginLeft: "var(--space-3)" }}>
            {shown.length} of {items.length} shown
          </span>
        ) : null}
      </div>

      {filtersOpen ? (
        <Card
          padding="var(--space-5)"
          style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}
        >
          <FacetRow
            title="context"
            values={contextsOf(items)}
            selected={picked.context}
            onToggle={(value) => setPicked(toggleFacet(picked, "context", value))}
          />
          <FacetRow
            title="size"
            values={SIZES}
            selected={picked.size}
            onToggle={(value) => setPicked(toggleFacet(picked, "size", value))}
          />
          <FacetRow
            title="energy"
            values={ENERGIES}
            selected={picked.energy}
            onToggle={(value) => setPicked(toggleFacet(picked, "energy", value))}
          />
          <FacetRow
            title="urgency"
            values={["overdue", "now", "soon"]}
            selected={picked.urgency}
            onToggle={(value) => setPicked(toggleFacet(picked, "urgency", value))}
          />
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              borderTop: "1px solid var(--border-subtle)",
              paddingTop: "var(--space-4)",
            }}
          >
            <span className="hb-meta">
              {shown.length} of {items.length} shown
            </span>
            {active > 0 ? (
              <button
                type="button"
                onClick={() => setPicked(NO_FACETS)}
                style={{
                  font: "var(--type-body-sm)",
                  background: "none",
                  border: "none",
                  color: "var(--text-link)",
                  cursor: "pointer",
                  display: "flex",
                  alignItems: "center",
                  gap: "var(--space-2)",
                }}
              >
                <Icon name="x" size={14} />
                Clear
              </button>
            ) : null}
          </div>
        </Card>
      ) : null}

      {/* The one thing colour says here, stated once as a key. `calm` is
          absent on purpose: it is the default and has no swatch. */}
      <div
        style={{
          display: "flex",
          gap: "var(--space-5)",
          flexWrap: "wrap",
          alignItems: "center",
        }}
      >
        {LEGEND.map((urgency) => (
          <span
            key={urgency}
            style={{ display: "flex", alignItems: "center", gap: "var(--space-2)" }}
          >
            <span
              style={{
                width: 3,
                height: 12,
                borderRadius: 2,
                background: URGENCY_EDGE[urgency],
              }}
            />
            <span className="hb-meta">{URGENCY_LABEL[urgency]}</span>
          </span>
        ))}
      </div>

      {ordered.length === 0 ? (
        <Card padding="var(--space-3)">
          <EmptyState
            icon="search"
            headingLevel={2}
            title="Nothing matches"
            body="No startable action carries every facet you picked."
          />
        </Card>
      ) : (
        // Wrapping columns, not a sideways-scrolling row: they flow onto as
        // many lines as the width needs, in reading order. Each column is
        // capped at `COLUMN_CAP` cards so one fat column cannot set the
        // height of a whole line and strand the others in whitespace — and
        // the cap is the honest one for this surface anyway, since the top
        // few of a column is what "what's next" is asking about. The rest is
        // one click away and the count never lies about what is hidden.
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: "var(--space-6)",
            alignItems: "flex-start",
          }}
        >
          {ordered.map(([key, columnItems]) => {
            const isOpen = expanded.has(key);
            const isCollapsed = collapsed.has(key);
            const visible = isOpen ? columnItems : columnItems.slice(0, COLUMN_CAP);
            const hidden = columnItems.length - visible.length;
            return (
              <div
                key={key}
                style={
                  isCollapsed
                    ? {
                        // A collapsed column stays exactly where it is in the
                        // board — but it stops claiming a full column's width,
                        // so its neighbours reflow around it instead of a
                        // wrapping line holding a slot that is only a header
                        // tall. Shrink-to-fit is what makes collapsing in
                        // place actually buy space.
                        flex: "0 0 auto",
                        display: "flex",
                        flexDirection: "column",
                      }
                    : {
                        flex: "1 1 240px",
                        minWidth: 240,
                        // Wide enough that a narrow window (where only one
                        // column fits beside the aside) fills its width
                        // instead of stranding a strip of empty page.
                        maxWidth: 380,
                        display: "flex",
                        flexDirection: "column",
                        gap: "var(--space-3)",
                      }
                }
              >
                {/* The header is the collapse control. A column you have ruled
                    out (wrong context, wrong energy) should cost one line, not
                    a screenful — and unlike the Filter panel this is
                    per-column and additive, so you can shut three and leave
                    the rest alone. The count stays readable while collapsed:
                    a shut column must still say how much is inside it. */}
                <button
                  type="button"
                  aria-expanded={!isCollapsed}
                  onClick={() => toggleIn(setCollapsed, key)}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "var(--space-3)",
                    width: "100%",
                    padding: "var(--space-2)",
                    background: "none",
                    border: "none",
                    borderBottom: `1px solid ${isCollapsed ? "var(--border-subtle)" : "transparent"}`,
                    textAlign: "left",
                    cursor: "pointer",
                  }}
                >
                  <Icon
                    name="chevron-down"
                    size={14}
                    style={{
                      color: "var(--text-muted)",
                      transform: isCollapsed ? "rotate(-90deg)" : "none",
                      transition: "transform var(--dur-fast) var(--ease-flit)",
                    }}
                  />
                  <span
                    style={{
                      flex: 1,
                      minWidth: 0,
                      font: "var(--type-body-strong)",
                      color: isCollapsed ? "var(--text-secondary)" : "var(--text-primary)",
                    }}
                  >
                    {key}
                  </span>
                  <span className="hb-meta">{columnItems.length}</span>
                </button>
                {isCollapsed
                  ? null
                  : visible.map((item) => (
                      <ItemCard
                        key={item.id}
                        item={item}
                        nowMs={nowMs}
                        selected={item.id === selectedId}
                        onOpen={() => onOpenItem(item.id)}
                      />
                    ))}
                {!isCollapsed && (hidden > 0 || isOpen) ? (
                  <button
                    type="button"
                    onClick={() => toggleIn(setExpanded, key)}
                    style={{
                      font: "var(--type-body-sm)",
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
      )}
    </>
  );
}
