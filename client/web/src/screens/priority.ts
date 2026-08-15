// `items.priority` (ADR-0009) carries Linear's own wire encoding verbatim
// (ADR-0008: "priority survives — it is human-set intent, not a Linear-ism"):
// 0 means "No priority" and 1..4 are Urgent, High, Medium, Low. That
// encoding is inverted (1 is the most urgent, not the least) *and* holed (0
// is not a value between Low and Urgent — it sorts after all four), so nothing
// in this app may sort or render the raw number directly. This is the web
// host's own copy of `client/core/src/task/item.rs`'s `Priority::rank`
// (the S1/Linear-era mirror's twin), kept here as a pure TS function since
// the frontier's ordering (issue #108's acceptance criterion: "Ordering is
// a pure function and is unit-tested") lives on this side of the wire, not
// in `hummingbird_domain` — the owned schema keeps `priority` a bare `i64`
// and leaves the display-rank opinion to consumers (ADR-0002).
//
// **Stays a literal TS function, not a live seam call — pinned instead.**
// `priorityRank` is read at module-evaluation time by `PRIORITY_OPTIONS`
// above, before `initDecisions()` is guaranteed to have resolved (the same
// constraint `field-vocabulary.ts`'s header states for its own literal
// arrays), so this cannot become a call through `decisions/seam.ts`.
// `hummingbird_core::decisions::frontier::priority_rank` is the ADR-0025
// canonical copy (M1-3, #501); `seam.test.ts` pins the two against each
// other via `priorityRankFromCore` so this copy cannot drift silently.

const LABELS: Record<number, string> = {
  0: "No priority",
  1: "Urgent",
  2: "High",
  3: "Medium",
  4: "Low",
};

const RANKS: Record<number, number> = {
  1: 0, // Urgent
  2: 1, // High
  3: 2, // Medium
  4: 3, // Low
  0: 4, // No priority — sorts last, never first
};

/** Every priority a form may set, in the order a reader expects to see them —
 * most urgent first, "No priority" last, i.e. `priorityRank` order and not the
 * raw ascending 0..4 the column stores. Values are strings because that is
 * what a `<select>` carries; the encoding itself stays here, so no screen has
 * to know that `1` is Urgent or that `0` sorts last. */
export const PRIORITY_OPTIONS: Array<{ value: string; label: string }> = Object.keys(RANKS)
  .map(Number)
  .sort((a, b) => priorityRank(a) - priorityRank(b))
  .map((raw) => ({ value: String(raw), label: priorityLabel(raw) }));

/** Rank for display ordering: most urgent first, "no priority" last. This
 * is the ordering the raw wire value does not give you — an unrecognised
 * value degrades to the same rank as "no priority" rather than throwing or
 * silently sorting as if it were Urgent. */
export function priorityRank(raw: number): number {
  return RANKS[raw] ?? RANKS[0];
}

/** The human-facing label — what the UI renders, never the raw number
 * (issue #108: "Render priority by its label, never by the raw number"). */
export function priorityLabel(raw: number): string {
  return LABELS[raw] ?? LABELS[0];
}

/** Whether `raw` is worth rendering a priority chip for at all — `false`
 * for an explicit 0 *and* for anything unrecognised, since both display as
 * "No priority" (`priorityLabel`) and a chip that reads "No priority" is
 * noise, not information. Callers should gate on this rather than
 * `raw !== 0` directly (PR #200 review: that comparison shows a chip for an
 * out-of-range value even though its label reads "No priority"). */
export function hasPriority(raw: number): boolean {
  return priorityLabel(raw) !== LABELS[0];
}
