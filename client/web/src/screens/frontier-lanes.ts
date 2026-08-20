// How Now's frontier columns are distributed across the board's width: how
// many vertical lanes the measured container affords, and which column goes in
// which lane.
//
// **Why this is TS and not Rust, under ADR-0025.** The decision here consumes
// a *measured pixel width* — a fact only the rendering runtime has, produced
// by a `ResizeObserver` over a DOM node, and meaningless on a phone whose
// columns are a single stack. `hummingbird_core::decisions::frontier::
// group_frontier` still owns everything about the columns themselves:
// membership, order, labels, the no-value bucket. This module never inspects
// an item. It takes a count of columns, a weight per column and a width, and
// answers where each one is drawn — the same carve-out ADR-0025's verdict
// table already makes for slider indices and viewport classes.
//
// **Why lanes at all.** The container used to be one `flex-wrap` row, which
// takes its line height from the TALLEST column in that line: two columns
// holding one item each, beside an `@computer` holding nine, each claimed a
// full track and left most of it blank. Lanes are the masonry shape instead —
// short columns stack under one another in the same lane — done in TS because
// CSS cannot deliver it: multi-column fills column-major, which breaks the
// fullest-first reading order `orderFrontier` establishes and is unassertable
// in jsdom, and grid masonry is not in stable browsers.

/** The narrowest a lane may be before the board drops one, and the gap
 * between lanes. `GAP` is the pixel twin of `--space-6`, the container's own
 * `gap` token: the two must move together, and there is no way to read a token
 * as a number here. `LANE_MIN` is the same 240 the columns' own
 * `flex-basis`/`min-width` has always used. */
const LANE_MIN = 240;
const GAP = 24;

/** How many lanes a container of this width affords, never more than there
 * are columns to fill them.
 *
 * `null` means *unmeasured* — the first layout pass before the observer has
 * run, and every jsdom test, which cannot lay out at all. The answer there is
 * one lane per column, which is exactly the pre-lanes layout: each column
 * alone in its own lane, in `group_frontier`'s order. A test that asserts the
 * board's structure keeps asserting the same thing it did, rather than
 * silently asserting a packing that no headless run could have produced. */
export function laneCountFor(widthPx: number | null, columnCount: number): number {
  if (columnCount <= 0) {
    return 0;
  }
  if (widthPx === null) {
    return columnCount;
  }
  // `+ GAP` on both sides because n lanes cost n-1 gaps: (w + gap) / (min +
  // gap) is the largest n satisfying n*min + (n-1)*gap <= w.
  const fits = Math.floor((widthPx + GAP) / (LANE_MIN + GAP));
  return Math.min(Math.max(fits, 1), columnCount);
}

/** Which columns land in which lane, as indices into `weights`.
 *
 * Greedy, in the given order: each column goes to the lane with the least in
 * it so far, leftmost on a tie. Two properties follow, and both are the point.
 * The first `laneCount` columns fan across the top exactly as the wrapping row
 * put them — every lane starts empty, so each takes the next one — so the
 * fullest columns still read left to right along the first line. And a short
 * column then stacks under whichever lane is currently shortest instead of
 * opening a track of its own.
 *
 * `weights` are rendered rows, not item counts: what costs vertical space is
 * what is on screen, so a collapsed column weighs its header alone. The caller
 * computes them, because only it knows what it is about to draw. */
export function packLanes(weights: readonly number[], laneCount: number): number[][] {
  const lanes: number[][] = Array.from({ length: Math.max(laneCount, 0) }, () => []);
  if (lanes.length === 0) {
    return lanes;
  }
  const totals = new Array<number>(lanes.length).fill(0);
  weights.forEach((weight, index) => {
    let pick = 0;
    for (let lane = 1; lane < totals.length; lane += 1) {
      // Strictly less, so an equal total leaves the leftmost lane the winner.
      if (totals[lane] < totals[pick]) {
        pick = lane;
      }
    }
    lanes[pick].push(index);
    totals[pick] += weight;
  });
  return lanes;
}
