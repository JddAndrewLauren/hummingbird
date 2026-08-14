// Now's centre column: the frontier as wrapping columns (#402, ADR-0021), and
// since the triage section was dissolved into them, the unsorted captures too
// — same columns, same axis, each capture marked with its `triage` stage chip
// and sorted under the startable actions of whatever column it lands in.
// Everything decidable lives in `frontier-columns.ts`; this file threads state
// through it and paints the result — the split every `screens/*` module keeps.
//
// The card is local rather than a sixteenth entry in `components/domain/`: it
// is used by this surface and no other, and `ItemRow` — which four screens
// share — stays exactly as it is for Triage, Done, Ledger and Now's own
// Blocked section. Two components because they have genuinely different
// densities and affordances, not a variant flag on one.

import { useState } from "react";
import type { ButtonHTMLAttributes, CSSProperties } from "react";
import { Badge } from "../components/core/Badge";
import { Card } from "../components/core/Card";
import { Icon } from "../components/core/Icon";
import { MarkDoneButton } from "../components/domain/MarkDoneButton";
import { StageBadge } from "../components/domain/StageBadge";
import { EmptyState } from "../components/feedback/EmptyState";
import { FRONTIER_AXES, groupFrontier, type FrontierAxis } from "./frontier-columns";
import {
  applyFacets,
  contextsOf,
  ENERGIES,
  facetCount,
  NO_FACETS,
  SIZES,
  toggleFacet,
  URGENCIES,
  type Facet,
  type FacetSelection,
} from "./frontier-facets";
import { orderFrontier } from "./frontier-order";
import { orderTriage } from "./triage-order";
import {
  readCollapsedColumns,
  readFrontierAxis,
  writeCollapsedColumns,
  writeFrontierAxis,
} from "./frontier-prefs";
import { canMarkDone } from "./item-actions";
import { hasPriority, priorityLabel } from "./priority";
import { energyIcon, energyTitle, levelColor, sizeIcon, sizeTitle } from "./size-energy";
import type { StorageLike } from "./storage";
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

/** The colour the urgency *word* is painted, which is deliberately **not**
 * `URGENCY_EDGE`.
 *
 * The word exists because ADR-0021 decision 2 makes colour non-load-bearing —
 * it is the channel for a reader who cannot use the swatch. Painting it in the
 * very colour it compensates for defeats that, and measurably so: at
 * `hb-meta`'s 11px on `--surface-card` in light mode, `--urgency-soon`
 * (`amber-500`) is ~2.4:1 and `--urgency-now` (`ember-500`) ~3.1:1, against
 * WCAG AA's 4.5:1 for small text. Stepping down the ramp does not rescue it
 * either — `amber-600` is still only ~3.5:1.
 *
 * So the swatch carries the colour and the word carries the information, in
 * text colours meant for text. `overdue` keeps an emphatic one because
 * `--status-danger-fg` is the repo's own "this is bad" text token and clears AA
 * comfortably (~8.5:1); the rest read as ordinary meta. */
const URGENCY_TEXT: Record<Urgency, string> = {
  overdue: "var(--status-danger-fg)",
  now: "var(--text-secondary)",
  soon: "var(--text-secondary)",
  calm: "var(--text-muted)",
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
        // was sitting next to. The mark is `accent`'s ember-tinted BORDER above
        // and `aria-current`, deliberately not a fill: the design system's card
        // rule is "the one card that is the answer on screen gets `accent` (an
        // ember-tinted border), not a fill", and ADR-0021 asks only that the
        // card "stays marked" without prescribing how.
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
          borderRadius: "var(--radius-xs)",
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
              screen-reader user does not reliably get (ADR-0021 decision 2) —
              and in `URGENCY_TEXT`, not the swatch colour, for the contrast
              reason recorded there. */}
          <span className="hb-meta" style={{ color: URGENCY_TEXT[urgency] }}>
            {URGENCY_LABEL[urgency]}
          </span>
          {/* "Ready" is the default and says nothing at card size — the stage
              chip earns its width only once the item is already running, or
              (since the captures joined these columns) not yet sorted. That
              chip IS the triage label: a capture is marked by the app's one
              stage vocabulary rather than by a badge invented for this
              surface, so the word on the card, on the Triage screen's rows and
              in the funnel are the same word. It is also not a fourth meaning
              for colour — stage is one of the three things the design system
              lets a coloured pill encode. */}
          {item.stage === "ready" ? null : <StageBadge stage={item.stage} />}
          {/* The level ramp, on the surface ADR-0021 decision 2 reserved for
              urgency. ADR-0024 narrows that: the *card's own* colour still
              means urgency and nothing else, and these badges are `ItemRow`'s
              vocabulary inherited unchanged — the same way `StageBadge` and
              the priority label already arrive here carrying their own
              colour. The honest cost, accepted: an amber mark on a card can
              now mean "due soon" (the leading edge) or "normal size" (this
              badge). Absent stays absent — a dense column is not the place to
              draw an unjudged dimension; `ItemDetailPanel` is. */}
          {item.size ? (
            <Badge
              mono
              icon={sizeIcon(item.size)}
              title={sizeTitle(item.size)}
              style={{ color: levelColor(item.size) }}
            />
          ) : null}
          {item.energy ? (
            <Badge
              mono
              icon={energyIcon(item.energy)}
              title={energyTitle(item.energy)}
              style={{ color: levelColor(item.energy) }}
            />
          ) : null}
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

/** What a `controlStyle` control looks like under the pointer.
 *
 * A picked control is `quiet`-shaped, so it darkens one accent step; an
 * unpicked one is ghost-shaped, so it takes `--surface-quiet` and firms its
 * text. Both per the design system's hover contract. */
function controlHoverStyle(selected: boolean) {
  return selected
    ? { background: "color-mix(in oklab, var(--accent-quiet) 70%, var(--accent) 12%)" }
    : { background: "var(--surface-quiet)", color: "var(--text-primary)" };
}

/** A control that honours the design system's hover contract.
 *
 * These stay hand-rolled rather than becoming `components/core/Button`: every
 * one of them is a *toggle* carrying `aria-pressed` or `aria-expanded`, and the
 * column header is a full-width button inside an `h2` whose own font and left
 * alignment the library's centred fixed-size skins do not fit. What they must
 * not do is skip the hover state — "a hovered thing gets *more* solid, not
 * less" — which every one of these controls was missing, and which is the only
 * thing this wrapper adds. */
function ControlButton({
  baseStyle,
  hoverStyle,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  baseStyle: CSSProperties;
  hoverStyle: CSSProperties;
}) {
  const [hover, setHover] = useState(false);
  return (
    <button
      {...rest}
      type="button"
      onMouseEnter={(event) => {
        setHover(true);
        rest.onMouseEnter?.(event);
      }}
      onMouseLeave={(event) => {
        setHover(false);
        rest.onMouseLeave?.(event);
      }}
      style={{
        transition:
          "background var(--dur-fast) var(--ease-flit), color var(--dur-fast) var(--ease-flit)",
        ...baseStyle,
        ...(hover ? hoverStyle : null),
      }}
    >
      {children}
    </button>
  );
}

const FACET_LABEL: Record<Facet, string> = {
  context: "context",
  size: "size",
  energy: "energy",
  urgency: "urgency",
};

function FacetRow({
  facet,
  values,
  selected,
  onToggle,
}: {
  facet: Facet;
  values: readonly string[];
  selected: ReadonlySet<string>;
  onToggle: (value: string) => void;
}) {
  if (values.length === 0) {
    return null;
  }
  return (
    <div style={{ display: "flex", gap: "var(--space-3)", alignItems: "baseline", flexWrap: "wrap" }}>
      <span className="hb-meta" style={{ minWidth: 64 }}>
        {FACET_LABEL[facet]}
      </span>
      {values.map((value) => (
        <ControlButton
          key={value}
          aria-pressed={selected.has(value)}
          // The chip's visible text is the bare value, which does not say which
          // facet it belongs to — and on the `context` axis it collides exactly
          // with a column heading of the same name. The accessible name carries
          // the facet, so "@garden" the filter and "@garden" the column are
          // distinguishable by ear as well as by eye.
          aria-label={`${FACET_LABEL[facet]} ${value}`}
          onClick={() => onToggle(value)}
          baseStyle={{
            ...controlStyle(selected.has(value)),
            minHeight: "var(--row-height)",
            padding: "var(--space-2) var(--space-4)",
            borderRadius: "var(--radius-pill)",
          }}
          hoverStyle={controlHoverStyle(selected.has(value))}
        >
          {value}
        </ControlButton>
      ))}
    </div>
  );
}

export function FrontierColumns({
  frontier,
  triage,
  projects,
  nowMs,
  selectedItemId,
  onOpenItem,
  onAct,
  storage,
}: {
  frontier: readonly TaskItemDTO[];
  /** `TaskState.triageInbox` — the unsorted captures, grouped into the same
   * columns as the frontier rather than into a section of their own. They
   * carry no axis value until somebody triages them, so on every axis they
   * land in the no-value column; a capture a sweeper *did* set a context on
   * lands in that context's column, which is the point of grouping them at
   * all rather than stacking them somewhere separate. */
  triage: readonly TaskItemDTO[];
  projects: readonly ProjectDTO[];
  nowMs: number;
  selectedItemId: string | null;
  onOpenItem: (itemId: string) => void;
  onAct: (itemId: string, action: "complete") => void;
  /** The same injected storage `NowScreen` resolves once for the triage
   * section and the ranked region — threaded rather than read here, so the
   * screen has one storage seam and not three. */
  storage?: StorageLike;
}) {
  // Seeded from storage on first render, then written on every change. `useState`'s
  // initialiser runs once, which is what makes a reload restore rather than a
  // re-render re-read.
  const [axis, setAxis] = useState<FrontierAxis>(() => readFrontierAxis(storage));
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() =>
    readCollapsedColumns(storage),
  );
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set<string>());
  const [filtersOpen, setFiltersOpen] = useState(false);
  // Deliberately NOT persisted — see `frontier-prefs.ts`. Opening Now to a
  // filtered board you would misread as an empty frontier is the failure this
  // avoids.
  const [picked, setPicked] = useState<FacetSelection>(NO_FACETS);

  // `orderFrontier` unchanged, applied once before filtering and grouping —
  // neither reorders, so the within-column rule is `orderFrontier` and there is
  // no second ordering function.
  //
  // The captures are appended AFTER the whole ordered frontier, and that
  // single concatenation is the entire implementation of "triage sits below
  // the promoted items": `groupFrontier` preserves input order inside every
  // bucket, so whichever column a capture lands in, it lands under that
  // column's startable actions. No third ordering function, and no per-column
  // partition step — the rule the old triage *section* enforced by position on
  // the page is now enforced by position in this array.
  //
  // Their own order among themselves stays `orderTriage` (oldest capture
  // first), the same rule the Triage screen sorts by, so the two surfaces
  // never disagree about which capture is next.
  const ordered = [...orderFrontier(frontier), ...orderTriage(triage)];
  const shown = applyFacets(ordered, picked, nowMs);
  const columns = groupFrontier(shown, axis, projects);
  const activeFacets = facetCount(picked);

  const pickAxis = (next: FrontierAxis) => {
    setAxis(next);
    writeFrontierAxis(storage, next);
    // Both per-column states are keyed by the column's own label, and
    // switching the axis re-labels every column — the old keys would then
    // apply to whatever happened to share a name. Cleared with the switch, the
    // same instinct as ADR-0015 discarding a pane override when its computed
    // band changes.
    setCollapsed(new Set<string>());
    writeCollapsedColumns(storage, new Set<string>());
    setExpanded(new Set<string>());
  };

  // The keys a column could legitimately have on this axis, so a write can
  // prune dead ones. Without pruning, a collapsed column whose label stops
  // existing — the last `@garden` action done, a project archived or renamed —
  // keeps its entry in storage forever: the very "an override map would accrete
  // keys for panes that no longer exist" failure ADR-0021 decision 5 cites as
  // its reason to stay out of the `settings` table. Device-local does not make
  // unbounded growth fine, and a column of that name appearing again later would
  // come back collapsed for a reason the reader cannot see.
  // `questions/collapse.ts`'s `writeCollapseOverride(…, livePaneKeys)` is the
  // in-repo shape for this.
  //
  // Derived from the **unfiltered** frontier, which is the whole subtlety: a
  // column the live filter happens to be hiding is not dead, and pruning
  // against `columns` would silently forget it was shut.
  const liveKeys = new Set(
    groupFrontier(ordered, axis, projects).map((column) => column.value ?? ""),
  );

  const toggleCollapsed = (key: string) => {
    const next = new Set<string>();
    for (const entry of collapsed) {
      if (liveKeys.has(entry)) {
        next.add(entry);
      }
    }
    if (collapsed.has(key)) {
      next.delete(key);
    } else {
      next.add(key);
    }
    // Computed here rather than inside a `setCollapsed` updater on purpose: an
    // updater must be pure, and StrictMode calls it twice, so the storage write
    // does not belong in one. Reading `collapsed` directly is safe because each
    // header click is its own React event — two collapses cannot batch into one
    // pass, which is the only case an updater would buy anything for.
    setCollapsed(next);
    writeCollapsedColumns(storage, next);
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
          <ControlButton
            key={entry}
            aria-pressed={axis === entry}
            onClick={() => pickAxis(entry)}
            baseStyle={controlStyle(axis === entry)}
            hoverStyle={controlHoverStyle(axis === entry)}
          >
            {AXIS_LABEL[entry]}
          </ControlButton>
        ))}

        {/* The axis switch is permanent chrome and the filter hides behind a
            button: filtering is the occasional gesture, so only one of the two
            earns permanent space. The button still carries its own count,
            because a filtered board that looks unfiltered is a lie. */}
        <ControlButton
          aria-expanded={filtersOpen}
          onClick={() => setFiltersOpen(!filtersOpen)}
          baseStyle={{ ...controlStyle(activeFacets > 0), marginLeft: "var(--space-4)" }}
          hoverStyle={controlHoverStyle(activeFacets > 0)}
        >
          <Icon name="search" size={14} />
          Filter
          {/* The count's pill is `neutral`, not `brand`. A coloured pill here
              would be the only one on the surface encoding neither stage, tier
              nor urgency — against the design system's colour rule, and against
              decision 2 above, which is the whole reason colour says one thing
              here. */}
          {activeFacets > 0 ? (
            <Badge mono tone="neutral">
              {activeFacets}
            </Badge>
          ) : null}
        </ControlButton>
        {/* Only when the panel is shut — open, the panel states the same count
            at the foot of the chips it belongs to. */}
        {activeFacets > 0 && !filtersOpen ? (
          <span className="hb-meta" style={{ marginLeft: "var(--space-3)" }}>
            {`${shown.length} of ${ordered.length} shown`}
          </span>
        ) : null}
      </div>

      {filtersOpen ? (
        <Card
          padding="var(--space-5)"
          style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}
        >
          <FacetRow
            facet="context"
            values={contextsOf(ordered)}
            selected={picked.context}
            onToggle={(value) => setPicked(toggleFacet(picked, "context", value))}
          />
          <FacetRow
            facet="size"
            values={SIZES}
            selected={picked.size}
            onToggle={(value) => setPicked(toggleFacet(picked, "size", value))}
          />
          <FacetRow
            facet="energy"
            values={ENERGIES}
            selected={picked.energy}
            onToggle={(value) => setPicked(toggleFacet(picked, "energy", value))}
          />
          <FacetRow
            facet="urgency"
            values={URGENCIES}
            selected={picked.urgency}
            onToggle={(value) => setPicked(toggleFacet(picked, "urgency", value))}
          />
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              gap: "var(--space-4)",
              borderTop: "1px solid var(--border-subtle)",
              paddingTop: "var(--space-4)",
            }}
          >
            <span className="hb-meta">
              {`${shown.length} of ${ordered.length} shown`}
            </span>
            {activeFacets > 0 ? (
              <ControlButton
                onClick={() => setPicked(NO_FACETS)}
                baseStyle={{
                  font: "var(--type-body-sm)",
                  minHeight: "var(--row-height)",
                  padding: "0 var(--space-3)",
                  borderRadius: "var(--radius-control)",
                  background: "none",
                  border: "none",
                  color: "var(--text-link)",
                  cursor: "pointer",
                  display: "flex",
                  alignItems: "center",
                  gap: "var(--space-2)",
                }}
                hoverStyle={{ background: "var(--surface-quiet)" }}
              >
                <Icon name="x" size={14} />
                Clear
              </ControlButton>
            ) : null}
          </div>
        </Card>
      ) : null}

      {/* The one thing colour says here, stated once as a key. */}
      <div style={{ display: "flex", gap: "var(--space-5)", flexWrap: "wrap", alignItems: "center" }}>
        {LEGEND.map((urgency) => (
          <span key={urgency} style={{ display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
            <span
              aria-hidden="true"
              style={{
                width: 3,
                height: 12,
                borderRadius: "var(--radius-xs)",
                background: URGENCY_EDGE[urgency],
              }}
            />
            <span className="hb-meta">{URGENCY_LABEL[urgency]}</span>
          </span>
        ))}
      </div>

      {/* An empty result says so, rather than rendering as an empty frontier —
          "nothing matches what you picked" and "nothing is startable" are very
          different facts and must not look alike. */}
      {columns.length === 0 ? (
        <Card padding="var(--space-3)">
          <EmptyState
            icon="search"
            headingLevel={2}
            title="Nothing matches"
            body="No startable action carries every facet you picked."
          />
        </Card>
      ) : null}

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
          const isCollapsed = collapsed.has(key);
          const visible = isOpen ? column.items : column.items.slice(0, COLUMN_CAP);
          const hidden = column.items.length - visible.length;
          return (
            <div
              key={key}
              style={
                isCollapsed
                  ? {
                      // A collapsed column keeps exactly its place in the board
                      // — but it stops claiming a full column's width, so its
                      // neighbours reflow around it instead of a wrapping line
                      // holding a slot that is only a header tall. Shrink-to-fit
                      // is what makes collapsing *in place* actually buy space,
                      // and it is the same wrinkle as the no-sideways-scroll
                      // fix: a wrapping row takes its height from the tallest
                      // column in its line.
                      flex: "0 0 auto",
                      display: "flex",
                      flexDirection: "column",
                    }
                  : {
                      flex: "1 1 240px",
                      // See `layout.tsx`'s `Column`: a fixed minimum is an
                      // overflow below its own value, not a floor.
                      minWidth: "min(240px, 100%)",
                      // Wide enough that a narrow window — where only one column
                      // fits beside the aside — fills its width instead of
                      // stranding a strip of empty page.
                      maxWidth: 380,
                      display: "flex",
                      flexDirection: "column",
                      gap: "var(--space-3)",
                    }
              }
            >
              {/* The header is the collapse control. A column you have ruled out
                  (wrong context, wrong energy) should cost one line, not a
                  screenful — and unlike the filter this is per-column and
                  additive, so you can shut three and leave the rest alone.
                  The count sits *beside* the heading rather than inside it, so
                  neither the heading's nor the button's accessible name reads
                  "@computer 4" — the number is meta about the column, not part
                  of what the column is called. */}
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "var(--space-3)",
                  borderBottom: `1px solid ${isCollapsed ? "var(--border-subtle)" : "transparent"}`,
                }}
              >
                <h2 style={{ margin: 0, flex: 1, minWidth: 0, font: "inherit" }}>
                  <ControlButton
                    aria-expanded={!isCollapsed}
                    onClick={() => toggleCollapsed(key)}
                    baseStyle={{
                      display: "flex",
                      alignItems: "center",
                      gap: "var(--space-3)",
                      width: "100%",
                      minHeight: "var(--row-height)",
                      padding: "var(--space-2)",
                      borderRadius: "var(--radius-control)",
                      background: "none",
                      border: "none",
                      textAlign: "left",
                      cursor: "pointer",
                      font: "var(--type-body-strong)",
                      color: isCollapsed ? "var(--text-secondary)" : "var(--text-primary)",
                    }}
                    hoverStyle={{
                      background: "var(--surface-quiet)",
                      color: "var(--text-primary)",
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
                    <span style={{ flex: 1, minWidth: 0 }}>{heading}</span>
                  </ControlButton>
                </h2>
                {/* The count stays readable while shut: a closed column must
                    still say how much is inside it. */}
                <span className="hb-meta" style={{ paddingRight: "var(--space-2)" }}>
                  {column.items.length}
                </span>
              </div>
              {isCollapsed
                ? null
                : visible.map((item) => (
                    <ItemCard
                      key={item.id}
                      item={item}
                      nowMs={nowMs}
                      selected={item.id === selectedItemId}
                      onOpen={() => onOpenItem(item.id)}
                      onComplete={canMarkDone(item) ? () => onAct(item.id, "complete") : undefined}
                    />
                  ))}
              {/* The count never lies about what is hidden. Offered only when
                  there is genuinely something to reveal or re-hide: an expanded
                  column that has since dropped to the cap (a filter picked, an
                  item completed) has `hidden === 0` and would otherwise keep a
                  "Show fewer" that changes nothing when clicked. */}
              {!isCollapsed && (hidden > 0 || (isOpen && column.items.length > COLUMN_CAP)) ? (
                <ControlButton
                  aria-expanded={isOpen}
                  // The visible text is deliberately terse, which leaves two
                  // columns hiding the same number of cards with two
                  // identically-named buttons and nothing tying either to its
                  // column. The accessible name carries the column, the same fix
                  // the facet chips needed.
                  aria-label={
                    isOpen ? `Show fewer in ${heading}` : `Show ${hidden} more in ${heading}`
                  }
                  onClick={() => toggleExpanded(key)}
                  baseStyle={{
                    font: "var(--type-body-sm)",
                    minHeight: "var(--row-height)",
                    background: "none",
                    border: "none",
                    borderRadius: "var(--radius-control)",
                    color: "var(--text-link)",
                    cursor: "pointer",
                    textAlign: "left",
                    padding: "0 var(--space-2)",
                  }}
                  hoverStyle={{ background: "var(--surface-quiet)" }}
                >
                  {isOpen ? "Show fewer" : `${hidden} more`}
                </ControlButton>
              ) : null}
            </div>
          );
        })}
      </div>
    </>
  );
}
