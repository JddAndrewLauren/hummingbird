// The frontier's facet filter (#403). Pure: the predicate never mutates an
// item, and the one time-varying facet — urgency — takes the clock as a
// parameter rather than reading one.
//
// The semantics are the ordinary ones, stated because getting them backwards is
// easy: **within** a facet the picked values are OR-ed, **across** facets they
// are AND-ed, and an unpicked facet means "no opinion" and matches everything.
// So picking `@computer` and `@phone` widens, while picking `@computer` and
// `quick` narrows.
//
// Nothing here partitions or colours: ADR-0021 decision 2 cut the prototype's
// six-bucket colour vocabulary, and urgency appears below only as a facet the
// reader can pick, never as a bucket an item is sorted into.

import type { TaskItemDTO } from "../store/protocol";
import { computeUrgency, type Urgency } from "./urgency";

/** The facets the filter panel offers. Note the overlap with the grouping axes
 * is partial and deliberate: `project` is groupable but not filterable (a
 * project column already isolates one), and `urgency` is filterable but not
 * groupable (colour already carries it across whatever axis is live). */
export type Facet = "context" | "size" | "energy" | "urgency";

export const FACETS: readonly Facet[] = ["context", "size", "energy", "urgency"];

/** The schema's own vocabularies (`server/authority/src/schema.rs`):
 * `size IN ('quick','normal','deep')` (schema 7, ADR-0024 — the middle one
 * was `short` before it), `energy IN ('low','medium','high')`. */
export const SIZES: readonly string[] = ["quick", "normal", "deep"];
export const ENERGIES: readonly string[] = ["low", "medium", "high"];

/** `calm` is absent: it is the default, and a facet for "nothing pressing" is
 * a facet for "everything", which the unpicked state already means. */
export const URGENCIES: readonly Urgency[] = ["overdue", "now", "soon"];

/** Display token for the column and chip of items naming no value — `context`
 * is free text, so unlike `size` and `energy` its vocabulary is not closed and
 * the absent case needs a name of its own. */
export const NO_CONTEXT = "no context";

export type FacetSelection = Readonly<Record<Facet, ReadonlySet<string>>>;

export const NO_FACETS: FacetSelection = {
  context: new Set(),
  size: new Set(),
  energy: new Set(),
  urgency: new Set(),
};

/** Contexts actually present in the given items — the chip row offers what is
 * there rather than a fixed list, because `items.context` is free text in the
 * schema and the set of places a person works is theirs. The known vocabulary
 * leads in its own order, anything unrecognised follows alphabetically, and
 * `NO_CONTEXT` is last (the same last-place rule the unnamed column has). */
export function contextsOf(items: readonly TaskItemDTO[]): string[] {
  const known = ["@computer", "@phone", "@home", "@garden", "@errands", "@waiting"];
  const present = new Set(items.map((item) => item.context ?? NO_CONTEXT));
  const ordered = known.filter((context) => present.has(context));
  const extra = [...present]
    .filter((context) => context !== NO_CONTEXT && !known.includes(context))
    .sort();
  return [...ordered, ...extra, ...(present.has(NO_CONTEXT) ? [NO_CONTEXT] : [])];
}

/** How many values are picked across every facet — the Filter button's badge,
 * because a filtered board that looks unfiltered is a lie. */
export function facetCount(picked: FacetSelection): number {
  return FACETS.reduce((total, facet) => total + picked[facet].size, 0);
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

export function matchesFacets(
  item: TaskItemDTO,
  picked: FacetSelection,
  nowMs: number,
): boolean {
  if (picked.context.size > 0 && !picked.context.has(item.context ?? NO_CONTEXT)) {
    return false;
  }
  // `size` and `energy` have closed vocabularies and no "none" chip, so an
  // item naming neither is excluded the moment either facet is picked at all —
  // picking `quick` is a claim about the work's shape, and an unjudged item
  // makes no such claim.
  if (picked.size.size > 0 && !(item.size !== null && picked.size.has(item.size))) {
    return false;
  }
  if (picked.energy.size > 0 && !(item.energy !== null && picked.energy.has(item.energy))) {
    return false;
  }
  if (picked.urgency.size > 0 && !picked.urgency.has(computeUrgency(item.deadline, nowMs))) {
    return false;
  }
  return true;
}

/** The picked items, in the order given — the caller has already ordered with
 * `orderFrontier`, and filtering never reorders. */
export function applyFacets(
  items: readonly TaskItemDTO[],
  picked: FacetSelection,
  nowMs: number,
): TaskItemDTO[] {
  return items.filter((item) => matchesFacets(item, picked, nowMs));
}
