// The frontier board's columns: the frontier as wrapping columns (#402,
// ADR-0021), and since the triage section was dissolved into them, the
// "triage process" queue too (#357: Triage and Grilling together,
// CONTEXT.md) — same columns, same axis, each item marked with its own stage
// chip and sorted under the startable actions of whatever column it lands
// in. This is the board's own rendering of the SAME combined queue the Triage
// screen shows —
// `triage-process-order.ts`'s `triageProcessQueue` is the one function
// deciding membership and order for both, never a `stage ===` check here.
// Everything decidable lives in `frontier-columns.ts`; this file threads state
// through it and paints the result — the split every `screens/*` module keeps.
//
// The card is local rather than a sixteenth entry in `components/domain/`: it
// is used by this board and no other, and `ItemRow` — which four screens
// share — stays exactly as it is for Triage, Done, Ledger and the board's own
// Blocked section. Two components because they have genuinely different
// densities and affordances, not a variant flag on one.
//
// **One column can be drawn across more than one lane.** When the packing
// leaves lanes it had no column to fill — the urgency axis always does, three
// bands of one card never filling a lane between them — the column in the last
// packed lane runs on into them under a repeated, de-emphasised label.
// `frontier-lanes.ts` decides which column may and how far; this file decides
// whether that would show anything, because only it knows what the column
// holds.
//
// **Two surfaces mount this now**, via `FrontierBoard.tsx`: Now, and one
// project's dossier with the same board re-sliced to that project's items.
// What varies between them is exactly two props — `screen` (whose preference
// keys to use) and `axes` (which switcher buttons to offer). Nothing here
// knows which surface it is beyond those two.
//
// **The board writes, since #801: a card is dragged into a column and takes
// that column's value** (ADR-0021 decision 9). This file owns the wiring and
// nothing else — `frontier-drag.ts` is the gesture (a spring the card hangs
// from, the frozen column rects it hit-tests against, the edge auto-scroll),
// and `dropEdits` is the seam onto the core decision about which field a drop
// writes. What is here is what only the board knows: which column a DOM key
// names, which item a card draws, and that a drag must not end by opening the
// item panel.

import {
  Children,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import { Badge } from "../components/core/Badge";
import { Card } from "../components/core/Card";
import { Icon } from "../components/core/Icon";
import { MarkDoneButton } from "../components/domain/MarkDoneButton";
import { StageBadge } from "../components/domain/StageBadge";
import { EmptyState } from "../components/feedback/EmptyState";
import { ControlButton, SECTION_TOGGLE_HOVER, sectionToggleStyle } from "./ControlButton";
import {
  CALM_ORDERS,
  dropEdits,
  FRONTIER_AXES,
  groupFrontier,
  type CalmOrder,
  type FrontierAxis,
  type FrontierColumn,
} from "./frontier-columns";
import {
  beginDrag,
  cardAttrs,
  columnAttrs,
  settleLanding,
  type Landing,
  type LiveDrag,
} from "./frontier-drag";
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
import { columnCapFor, frontierLanes, laneWeightsFor } from "./frontier-lanes";
import { orderFrontier } from "./frontier-order";
import { triageProcessQueue } from "./triage-process-order";
import {
  type FrontierPrefsScreen,
  readCalmOrder,
  readCollapsedColumns,
  readFrontierAxis,
  writeCalmOrder,
  writeCollapsedColumns,
  writeFrontierAxis,
} from "./frontier-prefs";
import { canMarkDone } from "./item-actions";
import { hasPriority, priorityLabel } from "./priority";
import { energyIcon, energyTitle, levelColor, sizeIcon, sizeTitle } from "./size-energy";
import type { StorageLike } from "./storage";
import { computeUrgency, type Urgency } from "./urgency";
import type { ProjectDTO, TaskItemDTO, TriageEdits } from "../store/protocol";

const AXIS_LABEL: Record<FrontierAxis, string> = {
  context: "Context",
  project: "Project",
  size: "Size",
  energy: "Energy",
  urgency: "Urgency",
};

/** The calm-order control's two labels. Rendered only on the `urgency` axis
 * — the direction reads its `calm` column and nothing else, so on any other
 * axis the control would be a switch with no visible subject. */
const CALM_ORDER_LABEL: Record<CalmOrder, string> = {
  oldest: "Oldest first",
  newest: "Newest first",
};

/** Display text for the column of items naming no value on the live axis —
 * `frontier-columns.ts` returns `value: null` and leaves the words here. */
const NO_VALUE_LABEL: Record<FrontierAxis, string> = {
  context: "No context",
  project: "No project",
  size: "No size",
  energy: "No energy",
  // Unreachable, and pinned so by `urgency_never_yields_a_no_value_column`
  // in `hummingbird_core::decisions::frontier`: urgency is total, so an item
  // with no deadline — or one whose deadline will not parse — reads as
  // `calm` rather than as no value. Present because the map is exhaustive
  // over the axis vocabulary, and a plausible label is a better fallback
  // than a crash if that ever stops being true.
  urgency: "No urgency",
};

/** ADR-0021 decision 2: **colour encodes urgency and nothing else.** `calm`
 * takes the same hairline every card already has — the default is not a claim
 * worth colouring. The three coloured values used to be spelled out in a legend
 * above the board; the cards state the same thing in words on each card's own
 * meta line — for the three coloured bands — so the key was one row of
 * permanent chrome saying nothing new. */
const URGENCY_EDGE: Record<Urgency, string> = {
  overdue: "var(--urgency-overdue)",
  now: "var(--urgency-now)",
  soon: "var(--urgency-soon)",
  calm: "var(--border-subtle)",
};

/** The `calm` entry is no longer rendered on a card — the same ADR-0021
 * decision 2 reason `URGENCY_EDGE` gives it no colour. It stays in the record
 * because the type is `Record<Urgency, string>` and dropping the key buys
 * nothing. */
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

/** One lane's box. Shared because a column that runs on into the board's spare
 * width draws lanes of its own after the packed ones, and a continuation that
 * did not sit on the same grid as what it continues would not read as one. */
const LANE_STYLE = {
  flex: "1 1 240px",
  // See `layout.tsx`'s `Column`: a fixed minimum is an overflow below its own
  // value, not a floor.
  minWidth: "min(240px, 100%)",
  // Wide enough that a narrow window — where only one lane fits beside the
  // aside — fills its width instead of stranding a strip of empty page.
  maxWidth: 380,
  display: "flex",
  flexDirection: "column",
  gap: "var(--space-6)",
} as const;

/** A column's own box inside its lane. */
const COLUMN_STYLE = {
  // The lane owns the width now, and every column in it fills that width —
  // including a collapsed one, which used to shrink to fit so its neighbours
  // could reflow around the slot it stopped needing. A header that keeps its
  // lane's width stays a line you can find and reopen rather than a stub
  // floating beside a full column.
  width: "100%",
  display: "flex",
  flexDirection: "column",
  gap: "var(--space-3)",
} as const;

/** The "n more" / "Show fewer" control.
 *
 * Its own component because a column that runs on into the spare lanes puts it
 * at the foot of the LAST of them — where the reading actually ends — and a
 * second copy of this markup would be a second place to fix anything about it.
 * The count never lies about what is hidden, and the control is offered only
 * when there is genuinely something to reveal or re-hide: an expanded column
 * that has since dropped under the cap (a filter picked, an item completed)
 * has `hidden === 0` and would otherwise keep a "Show fewer" that changes
 * nothing when clicked. */
function RevealControl({
  heading,
  hidden,
  isOpen,
  onToggle,
}: {
  heading: string;
  hidden: number;
  isOpen: boolean;
  onToggle: () => void;
}) {
  return (
    <ControlButton
      aria-expanded={isOpen}
      // The visible text is deliberately terse, which leaves two columns hiding
      // the same number of cards with two identically-named buttons and nothing
      // tying either to its column. The accessible name carries the column, the
      // same fix the facet chips needed.
      aria-label={isOpen ? `Show fewer in ${heading}` : `Show ${hidden} more in ${heading}`}
      onClick={onToggle}
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
  );
}

/** The card's meta line, which draws **only if it has something on it**.
 *
 * Every entry on that line is conditional — stage says nothing when it is
 * `ready`, size/energy/priority/deadline/scheduled/pending each draw only when
 * set, and since the `calm` word went the urgency entry is conditional too. So
 * the ordinary minted action (calm, ready, nothing else judged yet) reaches
 * this with no children at all, and an empty flex row is not free: the card
 * body's own `gap` still pays for it, stranding blank space under the title.
 *
 * The count is taken from the children rather than from a predicate spelling
 * out the same eight conditions a second time — a predicate is a copy that
 * goes stale the first time an entry is added here and not there. */
function CardMeta({ children }: { children: ReactNode }) {
  if (Children.toArray(children).length === 0) return null;
  return (
    <span
      style={{
        display: "flex",
        alignItems: "center",
        gap: "var(--space-3)",
        flexWrap: "wrap",
      }}
    >
      {children}
    </span>
  );
}

/** The drag gesture's state on the board's side: one mutable box, written
 * by the gesture and read by the render. A `useState` initialiser rather
 * than a `useRef` for the reason `frontier-lanes`'s own boxes are —
 * `react-hooks/refs` refuses a `ref.current` read during render, and what is
 * wanted here is a stable mutable object that is not ref-shaped. Every write
 * goes through one of the helpers below for the same reason:
 * `react-hooks/immutability` reads an assignment to a `useState` value at the
 * call site as a bug, and it is right about every case but these. */
interface DragBox {
  /** Set the moment a gesture arms, cleared at every `pointerdown`, so the
   * click a drag ends with can be swallowed before it opens the panel.
   * Cleared unconditionally — including on the presses this file refuses to
   * carry — because a `moved` left standing swallows the NEXT click, and
   * the next click is usually the mark-done checkmark. */
  moved: boolean;
  /** The gesture in flight, so a second press on the same card can put the
   * first down rather than running two rAF loops over one element, and so
   * an unmount can end it at all. */
  live: LiveDrag | null;
  /** The card being held at the slot it landed in, waiting for the board to
   * re-render under it. See `frontier-drag.ts`'s `Landing`. */
  landing: Landing | null;
  /** The live settle animation's canceller. */
  cancel: (() => void) | null;
  /** The safety net: a write that never comes back must not leave a card
   * floating above the board for the rest of the session. */
  fallback: ReturnType<typeof setTimeout> | null;
  /** Refilled on every render, so a gesture holding only a column's DOM key
   * can find the column the drop decision needs. */
  columns: Map<string, FrontierColumn>;
}

/** How long a held card waits for the render that moves it. Long enough
 * that a slow write still animates, short enough that a failed one is not a
 * card stuck above the board. */
const LANDING_FALLBACK_MS = 5000;

function rememberColumns(box: DragBox, columns: readonly FrontierColumn[]): void {
  box.columns = new Map(columns.map((column) => [column.value ?? "", column]));
}

/** A press begins. Whatever the last one left behind goes now: the swallow
 * flag, the gesture still in flight, and any card still being held at the
 * slot it landed in. Two gestures writing one element is the failure this
 * prevents — the older one keeps its own springs and would still commit,
 * against a column the hand has since left. */
function startGesture(box: DragBox): void {
  box.moved = false;
  abandonDrag(box);
}

function holdGesture(box: DragBox, live: LiveDrag): void {
  box.live = live;
}

function armGesture(box: DragBox): void {
  box.moved = true;
}

function holdLanding(box: DragBox, landing: Landing): void {
  box.landing = landing;
  if (box.fallback !== null) clearTimeout(box.fallback);
  box.fallback = setTimeout(() => releaseLanding(box, true), LANDING_FALLBACK_MS);
}

/** Finish whatever is being held, if the board has moved it — from the
 * layout effect on every render, since the board cannot know which render
 * carries the write. A card the board has not moved yet keeps its hold and
 * its timer; only the fallback ends that wait unconditionally. */
function releaseLanding(box: DragBox, force = false): void {
  const landing = box.landing;
  if (!landing) return;
  const settled = settleLanding(landing, force);
  if (settled.status === "waiting") return;
  box.landing = null;
  if (box.fallback !== null) clearTimeout(box.fallback);
  box.fallback = null;
  // The outgoing settle resets its own element as it stops — a cancelled
  // FLIP that left its transform behind would strand that card offset, and
  // React never rewrites an inline style it did not set.
  box.cancel?.();
  box.cancel = settled.status === "done" ? settled.cancel : null;
}

/** Everything this board is still holding, on the way out: a gesture in
 * flight, a settle mid-animation, and the fallback timer. `loop`'s canceller
 * exists for exactly this, and until #801's review nothing stored it. */
function abandonDrag(box: DragBox): void {
  box.live?.abort();
  box.live = null;
  // A held card is put DOWN rather than forgotten: `releaseLanding` is what
  // takes the carried styling and the transform off it, and forgetting the
  // landing would strand it floating above the board.
  releaseLanding(box, true);
  box.cancel?.();
  box.cancel = null;
}

/** What one card's own handlers are, so `ItemCard` takes one prop rather
 * than three and a caller with no worker passes nothing at all. */
interface CardDrag {
  "data-hb-card": string;
  onPointerDown: (event: ReactPointerEvent<HTMLDivElement>) => void;
  onClickCapture: (event: ReactMouseEvent<HTMLDivElement>) => void;
}

function ItemCard({
  item,
  nowMs,
  selected,
  onOpen,
  onComplete,
  drag,
}: {
  item: TaskItemDTO;
  nowMs: number;
  selected: boolean;
  onOpen: () => void;
  onComplete?: () => void;
  /** The board's drag handlers for this card (#801), or absent when nothing
   * can be written — the "no worker, no affordance" rule this file already
   * keeps for the mark-done checkmark. */
  drag?: CardDrag;
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
      {...drag}
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
        <CardMeta>
          {/* Colour carries urgency, so the card says it in words too. Text
              rather than `ItemRow`'s `title` tooltip, which a keyboard or
              screen-reader user does not reliably get (ADR-0021 decision 2) —
              and in `URGENCY_TEXT`, not the swatch colour, for the contrast
              reason recorded there. `calm` is the exception: it makes no claim
              to carry, so it gets no word, exactly as it gets no colour. */}
          {urgency === "calm" ? null : (
            <span className="hb-meta" style={{ color: URGENCY_TEXT[urgency] }}>
              {URGENCY_LABEL[urgency]}
            </span>
          )}
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
              role="img"
              aria-label={sizeTitle(item.size)}
              title={sizeTitle(item.size)}
              style={{ color: levelColor(item.size) }}
            />
          ) : null}
          {item.energy ? (
            <Badge
              mono
              icon={energyIcon(item.energy)}
              role="img"
              aria-label={energyTitle(item.energy)}
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
        </CardMeta>
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
  grilling,
  draftItemIds,
  projects,
  nowMs,
  selectedItemId,
  onOpenItem,
  onAct,
  storage,
  screen,
  axes = FRONTIER_AXES,
  onTriage,
}: {
  frontier: readonly TaskItemDTO[];
  /** `TaskState.triageInbox` — the captured Triage items, grouped into the
   * same columns as the frontier rather than into a section of their own.
   * They carry no axis value until somebody triages them, so on every axis
   * they land in the no-value column; a capture a sweeper *did* set a
   * context on lands in that context's column, which is the point of
   * grouping them at all rather than stacking them somewhere separate. */
  triage: readonly TaskItemDTO[];
  /** `TaskState.grillingItems` — the "triage process" queue's second half
   * (#357, CONTEXT.md), grouped into the same columns as `triage`: this is
   * Now's collapsible triage area, and neither it nor the Triage screen may
   * filter by stage on its own — `triageProcessQueue` is the one function
   * both read. */
  grilling: readonly TaskItemDTO[];
  /** `TaskState.grillDraftItemIds` (#356) — decides which of `triage`/
   * `grilling` sort to the front of the combined queue. */
  draftItemIds: readonly string[];
  projects: readonly ProjectDTO[];
  nowMs: number;
  selectedItemId: string | null;
  onOpenItem: (itemId: string) => void;
  onAct: (itemId: string, action: "complete") => void;
  /** The same injected storage `NowScreen` resolves once for the triage
   * section and the ranked region — threaded rather than read here, so the
   * screen has one storage seam and not three. */
  storage?: StorageLike;
  /** Which key namespace this board's axis and collapsed-column preferences
   * live under (`frontier-prefs.ts`). Required, not defaulted: a board that
   * silently shared Now's keys would re-group Now when it was switched. */
  screen: FrontierPrefsScreen;
  /** The axes this surface offers, defaulting to the whole vocabulary. The
   * project board passes every axis but `project`, which is degenerate there —
   * one column, always. A subset is a *rendering* choice and nothing more: the
   * vocabulary itself stays the seam's (ADR-0025), `groupFrontier` is called
   * unchanged, and the stored axis is clamped against this list on read so a
   * value chosen on a wider board cannot group by a button that is not here. */
  axes?: readonly FrontierAxis[];
  /** The board's one write (#801, ADR-0021 decision 9): a card dragged into
   * a column takes that column's value. `FrontierBoard` passes its own
   * `onTriage` straight through, and the `destination` is always `null` —
   * a drag edits fields and never promotes a capture.
   *
   * Absent means the surface has no worker behind it, and then nothing
   * drags at all: the same "no worker, no affordance" rule the mark-done
   * checkmark already keeps. */
  onTriage?: (itemId: string, destination: null, edits: TriageEdits) => void;
}) {
  // Seeded from storage on first render, then written on every change. `useState`'s
  // initialiser runs once, which is what makes a reload restore rather than a
  // re-render re-read.
  const [axis, setAxis] = useState<FrontierAxis>(() => readFrontierAxis(storage, screen, axes));
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() =>
    readCollapsedColumns(storage, screen),
  );
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set<string>());
  const [calmOrder, setCalmOrder] = useState<CalmOrder>(() => readCalmOrder(storage, screen));
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
  // Their own order among themselves stays `triageProcessQueue`'s (drafts,
  // then Grilling, then oldest-capture-first Triage) — the same rule the
  // Triage screen sorts by, so the two surfaces never disagree about which
  // capture is next.
  const ordered = [...orderFrontier(frontier), ...triageProcessQueue(triage, grilling, draftItemIds).items];
  const shown = applyFacets(ordered, picked, nowMs);
  const [drag] = useState<DragBox>(() => ({
    moved: false,
    live: null,
    landing: null,
    cancel: null,
    fallback: null,
    columns: new Map(),
  }));
  // No dependency array on purpose: a card held at the slot it landed in is
  // waiting for whichever render moves it, and the board cannot know which
  // that is — so it checks every one, and `releaseLanding` answers "not
  // this one" for all but the render that carries the write.
  useLayoutEffect(() => {
    if (drag.landing !== null) releaseLanding(drag);
  });
  // A drag interrupted by an unmount — the reader navigates away, or opens
  // the Recall overlay, with the pointer still down — never gets its own
  // `pointerup`, so nothing else would ever stop its rAF loop or take the
  // tint off the column it lit.
  useEffect(() => () => abandonDrag(drag), [drag]);
  const columns = groupFrontier(shown, axis, projects, nowMs, calmOrder);
  rememberColumns(drag, columns);

  /** One card's gesture (#801). The handlers are built per card because the
   * item is what a drop decision reads, and a gesture is short enough that
   * closing over this render's `axis`/`projects`/`nowMs` is exactly right.
   *
   * What a drop writes is not decided here: `dropEdits` is the seam onto
   * `hummingbird_core::decisions::frontier::drop_edits` (ADR-0025), and
   * `null` from it is both "already in that column" and "that column refuses
   * this card" — which is all `allows` needs to know to leave a column
   * unlit. */
  const cardDrag = (item: TaskItemDTO): CardDrag | undefined => {
    if (!onTriage) return undefined;
    const editsFor = (key: string): TriageEdits | null => {
      const column = drag.columns.get(key);
      return column ? dropEdits(item, axis, column.value, projects, nowMs) : null;
    };
    return {
      ...cardAttrs(item.id),
      onPointerDown: (event) => {
        // FIRST, and on every press including the ones refused below: a
        // `moved` left standing from the last drag swallows this click, and
        // this click is as likely as not the mark-done checkmark.
        startGesture(drag);
        // A card with a write already in flight has nothing stable to move,
        // and the mark-done checkmark is a control of its own — a gesture
        // starting on either would steal a click that means something else.
        if (item.pending || event.button !== 0) return;
        if ((event.target as HTMLElement).closest("button")) return;
        const live = beginDrag(
          event.currentTarget as HTMLElement,
          item.id,
          event.nativeEvent,
          {
            // Resolved once, when the gesture arms: `dropEdits` is a wasm
            // crossing plus two `JSON.stringify`s over the whole project
            // list, and `frontier-drag.ts`'s rule 1 is that a gesture does
            // not do that sixty times a second. The mobile seam's
            // `droppable_columns` is the same answer, for the same reason.
            allowed: () =>
              new Set(
                [...drag.columns.keys()].filter((key) => editsFor(key) !== null),
              ),
            drop: (key) => {
              const edits = editsFor(key);
              if (edits) onTriage(item.id, null, edits);
            },
            scroller: () => document.querySelector<HTMLElement>(".hb-scroll"),
            onArm: () => armGesture(drag),
            onLand: (landing) => holdLanding(drag, landing),
          },
        );
        holdGesture(drag, live);
      },
      onClickCapture: (event) => {
        // The click a drag ends with would otherwise open the item panel on
        // top of the board the reader has just rearranged.
        if (!drag.moved) return;
        event.preventDefault();
        event.stopPropagation();
      },
    };
  };
  const activeFacets = facetCount(picked);

  const pickAxis = (next: FrontierAxis) => {
    setAxis(next);
    writeFrontierAxis(storage, screen, next);
    // Both per-column states are keyed by the column's own label, and
    // switching the axis re-labels every column — the old keys would then
    // apply to whatever happened to share a name. Cleared with the switch, the
    // same instinct as ADR-0015 discarding a pane override when its computed
    // band changes.
    setCollapsed(new Set<string>());
    writeCollapsedColumns(storage, screen, new Set<string>());
    setExpanded(new Set<string>());
  };

  const pickCalmOrder = (next: CalmOrder) => {
    setCalmOrder(next);
    writeCalmOrder(storage, screen, next);
    // No collapse or expansion state to clear: the direction re-orders one
    // column's cards and renames nothing, so every key still means what it
    // meant.
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
    groupFrontier(ordered, axis, projects, nowMs, calmOrder).map((column) => column.value ?? ""),
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
    writeCollapsedColumns(storage, screen, next);
  };

  // The board's own width, the one fact `frontier-lanes.ts` cannot derive.
  // `null` until something measures it, which is the honest resting value and
  // not a placeholder: the first paint has not laid out yet, and jsdom never
  // will. `useIsPhone.ts`'s doctrine applies verbatim — a runtime with no
  // `ResizeObserver` is not a narrow screen, it is a runtime that cannot
  // answer, and `frontierLanes(…, null)` answers it with the pre-lanes layout
  // rather than guessing a width. A component test therefore keeps seeing one
  // column per lane; a test of the *packing* asserts `packLanes` directly,
  // where no stub can be forgotten.
  const boardRef = useRef<HTMLDivElement>(null);
  const [boardWidth, setBoardWidth] = useState<number | null>(null);
  // The room under the board, which decides the column cap the same way the
  // width decides the lane count — the board's RESTING room, held from the
  // first measurement and thereafter tracked against the window rather than
  // re-read.
  //
  // Re-reading the board's own top is the obvious thing and is wrong. That top
  // moves: the item panel opens *above* these columns and pushes them some
  // 400px down, so a fresh reading there would shrink every column on the
  // board under the click that opened the panel — the movement the lane
  // packing goes to such lengths to avoid. Freezing it also fixes an
  // order-dependence that measuring afresh would keep: resize the window while
  // a panel is open and the smaller room would stick after it closed. So the
  // board runs past the fold while something is open, in a region that already
  // scrolls, and is exactly right at rest — the state the reader is in nearly
  // all the time.
  const [boardRoom, setBoardRoom] = useState<number | null>(null);
  const restingRoom = useRef<{ room: number; fold: number } | null>(null);
  useLayoutEffect(() => {
    const node = boardRef.current;
    if (!node) {
      return;
    }
    const measureRoom = () => {
      const fold = document.documentElement.clientHeight;
      const anchor = restingRoom.current;
      // Same fold-back to `null` as the width: jsdom answers zero to every box
      // and a board with no room under it has not been laid out either.
      if (!anchor) {
        const room = fold - node.getBoundingClientRect().top;
        if (room > 0) {
          restingRoom.current = { room, fold };
        }
        setBoardRoom(room || null);
        return;
      }
      // A taller window is that much more room, whatever has since opened
      // above the board.
      setBoardRoom(anchor.room + (fold - anchor.fold) || null);
    };
    measureRoom();
    // The board's own observer cannot see this one: the viewport can grow
    // taller with the board exactly as wide, and that is precisely the resize
    // that changes the cap.
    window.addEventListener("resize", measureRoom);
    // The initial read happens whether or not an observer can be built: a
    // browser mid-resize is the observer's job, but the first measurement is
    // this line's, and a layout effect is what makes it land before paint.
    // Zero is folded back into `null` rather than taken at face value — jsdom
    // reports every box as 0x0, and a board that is genuinely zero wide (an
    // ancestor still hidden) has not been laid out either. Both are "no
    // answer", and answering "one lane" to them would stack the whole board.
    setBoardWidth(node.offsetWidth || null);
    if (typeof ResizeObserver === "undefined") {
      return () => window.removeEventListener("resize", measureRoom);
    }
    const observer = new ResizeObserver(([entry]) => {
      setBoardWidth(entry.target.clientWidth || null);
    });
    observer.observe(node);
    return () => {
      window.removeEventListener("resize", measureRoom);
      observer.disconnect();
    };
  }, []);

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

  // Item counts and the cap, and nothing about what is collapsed or revealed —
  // `laneWeightsFor`'s own doc carries the two bugs that bought that rule, and
  // its signature is what keeps it. A board that rearranges itself under the
  // click that opened one column is the fault ADR-0021 decision 1's amendment
  // refuses for the bands' order: it moves for reasons the reader did not
  // cause.
  const columnCap = columnCapFor(boardRoom);
  const laneWeights = laneWeightsFor(
    columns.map((column) => column.items.length),
    columnCap,
  );
  // Fewer lanes than the width affords whenever the weights do not reach that
  // far — `packLanes` drops the ones nobody filled, and the survivors widen
  // into the space. That is what keeps the urgency axis, three slight bands in
  // front of one very full `calm`, from drawing two empty tracks.
  const { lanes, spare } = frontierLanes(laneWeights, boardWidth);

  // How much of the board's spare width the column in the last lane actually
  // takes. `frontier-lanes.ts` says which column may run on and how far it
  // could; only here is it known whether that would show anything — a column
  // inside its cap has nothing to continue, and a lane holding a repeated
  // heading and no cards is worse than the whitespace it replaced.
  //
  // Computed from the column's item count against the cap, both of which are
  // resting facts, so the number of lanes drawn does not move when a column is
  // collapsed or revealed. Shutting `calm` leaves its continuation lane
  // standing and empty; that is the same bargain the packing weights make —
  // transient, self-inflicted, undone by the click that caused it — and the
  // alternative is the board re-widthing under the reader's hand.
  const flow = (() => {
    if (!spare) {
      return null;
    }
    const column = columns[spare.column];
    const wanted = Math.ceil(column.items.length / columnCap) - 1;
    const extra = Math.min(spare.lanes, Math.max(wanted, 0));
    return extra > 0 ? { column: spare.column, extra } : null;
  })();

  /** How many lanes a column is drawn across: one, unless it is the one
   * running on into the spare width. */
  const partsOf = (columnIndex: number) =>
    flow?.column === columnIndex ? flow.extra + 1 : 1;

  /** The slice of a column drawn in its `part`-th lane. Shut, a column shows
   * its cap in each lane it has and defers the rest; open, it spreads
   * everything it holds evenly across them, so no lane runs far past its
   * neighbour. */
  const chunkFor = (columnIndex: number, part: number) => {
    const column = columns[columnIndex];
    if (expanded.has(column.value ?? "")) {
      const size = Math.ceil(column.items.length / partsOf(columnIndex));
      return column.items.slice(part * size, (part + 1) * size);
    }
    return column.items.slice(part * columnCap, (part + 1) * columnCap);
  };

  /** What a column shows in total, across every lane it occupies — which is
   * what "n more" counts against, not the one chunk the control sits under. */
  const shownIn = (columnIndex: number) => {
    const column = columns[columnIndex];
    return expanded.has(column.value ?? "")
      ? column.items.length
      : Math.min(column.items.length, columnCap * partsOf(columnIndex));
  };

  const headingFor = (column: (typeof columns)[number]) =>
    column.value === null ? NO_VALUE_LABEL[axis] : (column.label ?? `Project ${column.value}`);

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
        {axes.map((entry) => (
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

        {/* Only the `urgency` axis has a `calm` column to order, so this is
            the one control on the strip that comes and goes. It sits directly
            after the axis buttons because it is a modifier of the one just
            pressed, not a peer of them. */}
        {axis === "urgency" ? (
          // A labelled group, not two more loose toggles. The axis buttons
          // beside these use the identical `aria-pressed` treatment, so
          // without a name for the pair a screen reader hears seven
          // same-shaped controls in a row and nothing saying the last two
          // answer a different question.
          <div
            role="group"
            aria-label="Calm column order"
            style={{
              display: "flex",
              gap: "var(--space-2)",
              alignItems: "center",
              marginLeft: "var(--space-4)",
            }}
          >
            {CALM_ORDERS.map((entry) => (
              <ControlButton
                key={entry}
                aria-pressed={calmOrder === entry}
                onClick={() => pickCalmOrder(entry)}
                baseStyle={controlStyle(calmOrder === entry)}
                hoverStyle={controlHoverStyle(calmOrder === entry)}
              >
                {CALM_ORDER_LABEL[entry]}
              </ControlButton>
            ))}
          </div>
        ) : null}

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

      {/* Columns packed into vertical lanes, never a sideways-scrolling strip:
          as many lanes as the measured width affords, each a stack of whole
          columns, and this container adds no scroll of its own —
          `docs/SURFACES.md` records the triage cap as the only independent
          scroll container in the centre column, and that stays true (ADR-0021
          decision 3).

          It was one `flex-wrap` row, and a wrapping line takes its height from
          the tallest column in it: `@home` and `@errands` holding one item
          each sat beside a nine-item `@phone`, claiming a full track apiece
          and leaving most of it blank. Stacking them under the shortest lane
          is what `frontier-lanes.ts` decides; that module's header carries
          why the packing is TS and why it is not CSS.

          Two consequences worth naming. DOM order is lane-major, so keyboard
          order follows the visual stacks rather than reading across the board —
          which is what someone tabbing down a lane would expect, and what the
          wrapping row already did within a line. And the lanes do NOT move
          under a collapse or a reveal: the weights they are packed from are
          each column's resting height, so a toggle changes one column's height
          and no column's position. That was learned the hard way; the comment
          on `laneWeights` above carries the two bugs that taught it. */}
      <div
        ref={boardRef}
        style={{
          display: "flex",
          gap: "var(--space-6)",
          alignItems: "flex-start",
        }}
      >
        {lanes.map((lane, laneIndex) => (
          <div
            // The lane index is a legitimate key: lanes are positions on the
            // board, not identities, and the columns inside carry their own.
            key={laneIndex}
            style={LANE_STYLE}
          >
            {lane.map((columnIndex) => {
              const column = columns[columnIndex];
              const key = column.value ?? "";
              const heading = headingFor(column);
              const isOpen = expanded.has(key);
              const isCollapsed = collapsed.has(key);
              const visible = chunkFor(columnIndex, 0);
              // Against everything the column shows, not against this lane's
              // slice: a column running on into the spare width has already
              // shown the next chunk to the right.
              const hidden = column.items.length - shownIn(columnIndex);
              const runsOn = flow?.column === columnIndex;
              return (
                <div
                  key={key}
                  // A drop target, and named as one: the gesture finds every
                  // drawn part of a column by this attribute, and a reader
                  // moving by structure gets a group whose label is the
                  // column's own heading rather than an unnamed div.
                  {...columnAttrs(key, column.value)}
                  role="group"
                  aria-label={heading}
                  style={COLUMN_STYLE}
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
                        baseStyle={sectionToggleStyle(isCollapsed)}
                        hoverStyle={SECTION_TOGGLE_HOVER}
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
                          drag={cardDrag(item)}
                        />
                      ))}
                  {/* A column that runs on hands this to the foot of its last
                      lane instead — where its reading actually ends. */}
                  {!isCollapsed &&
                  !runsOn &&
                  (hidden > 0 || (isOpen && column.items.length > columnCap)) ? (
                    <RevealControl
                      heading={heading}
                      hidden={hidden}
                      isOpen={isOpen}
                      onToggle={() => toggleExpanded(key)}
                    />
                  ) : null}
                </div>
              );
            })}
          </div>
        ))}

        {/* The continuation lanes: one column, drawn on across the width the
            packing had no column to fill. Only ever the column in the last
            packed lane, so these sit immediately right of the chunk they
            continue and read as one column rather than as a new one.

            The label is a plain line of meta, deliberately NOT a second `h2`:
            this is the same column, and a heading here would put a duplicate
            entry in the outline and offer a collapse control that already
            exists to its left. The first chunk owns the heading, the count and
            the collapse; the last owns the reveal. */}
        {flow
          ? Array.from({ length: flow.extra }, (_, offset) => {
              const columnIndex = flow.column;
              const column = columns[columnIndex];
              const key = column.value ?? "";
              const heading = headingFor(column);
              const isOpen = expanded.has(key);
              const isCollapsed = collapsed.has(key);
              const part = offset + 1;
              const hidden = column.items.length - shownIn(columnIndex);
              return (
                <div key={`${key}-part-${part}`} style={LANE_STYLE}>
                  <div
                    // The same column, so the same drop target — a card
                    // released over a continuation lands in the column it
                    // continues.
                    {...columnAttrs(key, column.value)}
                    role="group"
                    aria-label={`${heading} continued`}
                    style={COLUMN_STYLE}
                  >
                    {/* Shut with the rest of the column: a label standing over
                        no cards names nothing, and the heading it continues is
                        two lanes to the left saying the same word. The lane
                        itself stays, holding its width so the board does not
                        reflow around a collapse. */}
                    {isCollapsed ? null : (
                      <p
                        className="hb-meta"
                        style={{
                          margin: 0,
                          // Sits on the same baseline as the real heading
                          // beside it, so the chunks line up across the board.
                          minHeight: "var(--row-height)",
                          display: "flex",
                          alignItems: "center",
                          padding: "0 var(--space-2)",
                        }}
                      >
                        {heading} continued
                      </p>
                    )}
                    {isCollapsed
                      ? null
                      : chunkFor(columnIndex, part).map((item) => (
                          <ItemCard
                            key={item.id}
                            item={item}
                            nowMs={nowMs}
                            selected={item.id === selectedItemId}
                            onOpen={() => onOpenItem(item.id)}
                            onComplete={
                              canMarkDone(item) ? () => onAct(item.id, "complete") : undefined
                            }
                            drag={cardDrag(item)}
                          />
                        ))}
                    {!isCollapsed &&
                    part === flow.extra &&
                    (hidden > 0 || (isOpen && column.items.length > columnCap)) ? (
                      <RevealControl
                        heading={heading}
                        hidden={hidden}
                        isOpen={isOpen}
                        onToggle={() => toggleExpanded(key)}
                      />
                    ) : null}
                  </div>
                </div>
              );
            })
          : null}
      </div>
    </>
  );
}
