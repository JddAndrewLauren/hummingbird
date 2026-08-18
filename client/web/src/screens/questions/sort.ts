import { orderPanes as orderPanesFromCore, samePaneIdentity as samePaneIdentityFromCore } from "../../decisions/seam";
import type { RankedPane } from "./contract";

// ADR-0015's cross-pane sort (#245) — **now a re-export of the decision
// seam** (ADR-0025, #533/M4), exactly the way `frontier-order.ts` became
// one at #501.
//
// The rule itself is `hummingbird_core::decisions::panes::sort`: five axes
// (answer state -> band -> withinBand -> declared question order -> subject
// key), total, pure, non-mutating. Read that module for the reasoning; this
// file exists only so `RankedRegion.tsx`, `registry.ts` and `sort.test.ts`
// keep importing the same two names from the same place.
//
// Both signatures are unchanged, including `questionOrder: readonly
// string[]` — the core's own `order_panes` takes question *names* rather
// than a closed vocabulary for precisely this reason, so a caller (this
// file's own suite included) can declare a synthetic order and the sort
// stays total over it.

/** Orders the ranked panes for display. Never mutates `panes`. */
export function orderPanes(
  panes: readonly RankedPane[],
  questionOrder: readonly string[],
): RankedPane[] {
  return orderPanesFromCore(panes, questionOrder);
}

/** Whether two ranked lists describe the same set of panes in the same
 * answer states — the one comparison `RankedRegion` re-samples its captured
 * order on, beyond a sync cycle. Deliberately **not** a full equality: band
 * and `withinBand` move on their own with the clock. */
export function samePaneIdentity(a: readonly RankedPane[], b: readonly RankedPane[]): boolean {
  return samePaneIdentityFromCore(a, b);
}
