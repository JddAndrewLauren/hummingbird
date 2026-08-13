// PROTOTYPE — throwaway. Delete with the rest of `now-prototype/`.
//
// The facts all three variants disagree about how to *present*. Kept here so
// the variants differ in structure rather than in what they can see: each one
// is free to use none, some or all of this.

import type { TaskItemDTO } from "../../store/protocol";
import { computeUrgency, type Urgency } from "../urgency";

export const NO_CONTEXT = "no context";

/** Contexts present in the given items, in the vocabulary's own order with
 * anything unrecognised appended, and `NO_CONTEXT` last. */
export function contextsOf(items: readonly TaskItemDTO[]): string[] {
  const known = ["@computer", "@phone", "@home", "@garden", "@errands", "@waiting"];
  const present = new Set(items.map((item) => item.context ?? NO_CONTEXT));
  const ordered = known.filter((context) => present.has(context));
  const extra = [...present]
    .filter((context) => context !== NO_CONTEXT && !known.includes(context))
    .sort();
  return [...ordered, ...extra, ...(present.has(NO_CONTEXT) ? [NO_CONTEXT] : [])];
}

export const SIZES = ["quick", "short", "deep"] as const;
export const ENERGIES = ["low", "medium", "high"] as const;

export type Facet = "context" | "size" | "energy" | "urgency";

export type FacetSelection = Record<Facet, ReadonlySet<string>>;

export const NO_FACETS: FacetSelection = {
  context: new Set(),
  size: new Set(),
  energy: new Set(),
  urgency: new Set(),
};

export function facetCount(picked: FacetSelection): number {
  return Object.values(picked).reduce((total, set) => total + set.size, 0);
}

export function toggleFacet(
  picked: FacetSelection,
  facet: Facet,
  value: string,
): FacetSelection {
  const next = new Set(picked[facet]);
  if (next.has(value)) {
    next.delete(value);
  } else {
    next.add(value);
  }
  return { ...picked, [facet]: next };
}

/** An empty set for a facet means "no opinion", so it matches everything;
 * within a facet the picked values are OR-ed and across facets AND-ed. */
export function matchesFacets(
  item: TaskItemDTO,
  picked: FacetSelection,
  nowMs: number,
): boolean {
  if (picked.context.size > 0 && !picked.context.has(item.context ?? NO_CONTEXT)) {
    return false;
  }
  if (picked.size.size > 0 && !(item.size && picked.size.has(item.size))) {
    return false;
  }
  if (picked.energy.size > 0 && !(item.energy && picked.energy.has(item.energy))) {
    return false;
  }
  if (picked.urgency.size > 0 && !picked.urgency.has(urgencyOf(item, nowMs))) {
    return false;
  }
  return true;
}

export type Lane =
  | "overdue"
  | "today"
  | "scheduled"
  | "quick"
  | "deep"
  | "rest";

export const LANE_TITLES: Record<Lane, string> = {
  overdue: "Overdue",
  today: "Due today",
  scheduled: "Scheduled today",
  quick: "Quick wins",
  deep: "Deep work",
  rest: "Everything else",
};

export const LANE_BLURBS: Record<Lane, string> = {
  overdue: "The world has already moved past these.",
  today: "Due before the day is out.",
  scheduled: "You picked today for these.",
  quick: "Under five minutes each.",
  deep: "Needs a clear run at it.",
  rest: "Startable, nothing pressing.",
};

export const LANE_ORDER: Lane[] = ["overdue", "today", "scheduled", "quick", "deep", "rest"];

/** Variant B's lane colours, shared so C can carry the same encoding on a
 * board card. One colour per lane, and a lane is a partition, so a coloured
 * edge always means exactly one thing. */
export const LANE_ACCENT: Record<Lane, string> = {
  overdue: "var(--urgency-overdue)",
  today: "var(--urgency-now)",
  scheduled: "var(--urgency-soon)",
  quick: "var(--status-done-fg)",
  deep: "var(--text-brand)",
  rest: "var(--border-strong)",
};

function isToday(day: string | null, nowMs: number): boolean {
  if (day === null) {
    return false;
  }
  const now = new Date(nowMs);
  const month = `${now.getMonth() + 1}`.padStart(2, "0");
  const date = `${now.getDate()}`.padStart(2, "0");
  return day.slice(0, 10) === `${now.getFullYear()}-${month}-${date}`;
}

/** First lane an item falls into — the lanes are a partition, tried in
 * `LANE_ORDER`, so nothing is shown twice and nothing is dropped. */
export function laneOf(item: TaskItemDTO, nowMs: number): Lane {
  const urgency = computeUrgency(item.deadline, nowMs);
  if (urgency === "overdue") {
    return "overdue";
  }
  if (urgency === "now" || isToday(item.deadline, nowMs)) {
    return "today";
  }
  if (isToday(item.scheduledDate, nowMs)) {
    return "scheduled";
  }
  if (item.size === "quick") {
    return "quick";
  }
  if (item.size === "deep") {
    return "deep";
  }
  return "rest";
}

export function urgencyOf(item: TaskItemDTO, nowMs: number): Urgency {
  return computeUrgency(item.deadline, nowMs);
}

/** Sort key shared by the variants that show one flat run of items:
 * in-progress first, then urgency, then priority, then title. */
export function byAttention(nowMs: number) {
  const urgencyRank: Record<Urgency, number> = { overdue: 0, now: 1, soon: 2, calm: 3 };
  return (a: TaskItemDTO, b: TaskItemDTO): number => {
    const progress = Number(b.stage === "in_progress") - Number(a.stage === "in_progress");
    if (progress !== 0) {
      return progress;
    }
    const urgency = urgencyRank[urgencyOf(a, nowMs)] - urgencyRank[urgencyOf(b, nowMs)];
    if (urgency !== 0) {
      return urgency;
    }
    const priority = (a.priority || 9) - (b.priority || 9);
    if (priority !== 0) {
      return priority;
    }
    return a.title < b.title ? -1 : 1;
  };
}
