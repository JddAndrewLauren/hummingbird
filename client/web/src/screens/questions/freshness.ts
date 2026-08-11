import type { FreshnessDTO } from "../../store/protocol";

// ADR-0015's one **shared** freshness reading, and deliberately the only one
// the shell owns.
//
// The *threshold* stays in each pane's own module beside its band function —
// the driver is the cost of a wrong answer (`waste.ts` calls 26h stale where
// `2 × cadence` would say 48h; `race.ts` keeps the plain `2 ×` at 12h), and a
// `staleAfterMs` field on the DTO is the design ADR-0015 rejects. What must
// NOT be per-pane is the reading of `"unknown"`, which is why the predicate
// itself lives here rather than being copied into each pane.

/** Whether an answer is old enough to say so, against the caller's own
 * threshold.
 *
 * **`"unknown"` is never fresh** — the whole reason `Freshness` is a tagged
 * union rather than a nullable age (`client/core/src/freshness.rs`, whose
 * `is_stale_beyond` answers `true` for `Unknown` against every threshold
 * including `i64::MAX`). A pane that had no stamp to measure has measured
 * nothing, and "we don't know how old this is" must never render as "this is
 * current". A second copy of this line is a second chance to write
 * `freshness.ageMs > threshold` alone and lose the arm. */
export function isStaleFreshness(freshness: FreshnessDTO, thresholdMs: number): boolean {
  return freshness.kind === "unknown" || freshness.ageMs > thresholdMs;
}
