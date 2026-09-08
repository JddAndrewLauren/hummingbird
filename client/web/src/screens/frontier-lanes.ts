// How Now's frontier columns are laid out against the board's measured box:
// how many vertical lanes its width affords, which column goes in which lane,
// and how many cards a column shows before its "n more" control — the last of
// those a question about the board's HEIGHT, and here for the same reason as
// the other two.
//
// **Why this is TS and not Rust, under ADR-0025.** The decision here consumes
// a *measured pixel width* — a fact only the rendering runtime has, produced
// by a `ResizeObserver` over a DOM node, and meaningless on a phone whose
// columns are a single stack. `columnCapFor` consumes the other axis of the
// same box and is the same kind of decision. `hummingbird_core::decisions::frontier::
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
//
// **Why the packing fills rather than balances.** It was greedy-balance: each
// column went to the lane holding least so far. That rested on an invariant
// no one had written down — that columns arrive fullest-first — because every
// lane starts empty, so the first `laneCount` columns each claimed a lane of
// their own before anything stacked. The urgency axis broke it: its columns
// arrive in *severity* order (`group_frontier`, ADR-0021 decision 1 as
// amended), and its shape is three near-empty bands in front of one very full
// `calm`. Greedy gave the three sparse bands a full lane each and stacked the
// 29-item column under the first of them, which is the layout this replaced.
//
// Filling in order fixes that at the cause: every lane aims at the same share
// of the total — never less than the tallest single column, which sets the
// board's height on its own — and takes columns while they bring it closer to
// that share, and lanes nobody reached are not drawn at all — so the survivors widen (the
// container's `flex: 1 1 240px`) instead of standing empty. The cost is that
// a lane is read top-down before the eye moves right — the column-major reading
// the paragraph above rejects for CSS columns. It is the right reading here
// and the wrong one there for the same reason: severity *is* a vertical
// order, and a fullest-first axis still fans out, because a heavy column
// fills its lane on its own and the next one starts fresh. Where it does bite
// is a run of uniformly tiny columns — four columns of one item over two
// lanes now read 1,2 / 3,4 rather than 1,3 / 2,4 — and that is accepted:
// nothing there is far enough down a lane to be missed.

/** What one card and one heading cost vertically, and the breathing room left
 * under the board. Pixel twins of what the cards actually render at — a card
 * measures 66px with a one-line title and 87 with two, over a `--space-3`
 * gap, and the typical card is the one-line one — so this is the same kind of
 * constant `GAP` is, and moves when the card's padding or type does.
 *
 * Erring low is deliberate. A cap one card too generous costs a scroll in a
 * region that already scrolls; a cap one card too mean is the dead space this
 * exists to close. */
export const CARD_ROW = 78;
const HEADING_ROW = 44;
const BOTTOM_GUTTER = 24;

/** The floor and the unmeasured answer for `columnCapFor`. `DEFAULT` is the
 * fixed cap the board carried before the height was measured at all: it is
 * what jsdom and the first paint see, so a component test asserts the same six
 * cards it always asserted. `MIN` keeps a short viewport — a laptop with the
 * capture surface open — showing a column rather than a heading and one card. */
const DEFAULT_COLUMN_CAP = 6;
const MIN_COLUMN_CAP = 4;

/** What each column costs the packing in rows: a heading, the cards it shows,
 * and the "n more" control if it has one.
 *
 * **It takes item counts and a cap, and nothing else — that is the whole
 * point.** There is deliberately no way to tell this function that a column is
 * collapsed or revealed, because weighing what is DRAWN made every toggle a
 * repack, and a repack moves columns the reader never touched: shutting
 * `overdue` slid `calm` into another lane, and revealing `calm` — 29 cards,
 * suddenly the heaviest thing on the board by a factor of five — pulled the
 * whole urgency board into one lane. Lanes are a function of the columns and
 * the measured box; a toggle changes one column's height and no column's
 * position. Anything wanting to reopen that has to change this signature,
 * which is the point of the signature.
 *
 * Rows rather than pixels because cards are close enough to uniform that
 * measuring each would buy precision the eye cannot see; the consequence is
 * that a column of long titles runs a little past its lane-mates. */
export function laneWeightsFor(itemCounts: readonly number[], columnCap: number): number[] {
  return itemCounts.map(
    (count) => 1 + Math.min(count, columnCap) + (count > columnCap ? 1 : 0),
  );
}

/** How many cards a column shows before it defers the rest to "n more", from
 * the height the board actually has under it.
 *
 * A fixed cap of six was two things at once: an honest editorial claim — the
 * top few of a column is what "what's next" is asking about — and, silently, a
 * layout guess. On a tall screen the guess was badly wrong: at 1400px the
 * urgency board stopped 638px above the fold with 23 items behind a control,
 * which is dead space the reader has to click to fill. The editorial claim
 * survives; what the column defers is now what genuinely does not fit.
 *
 * `null` is *unmeasured*, and the honest answer there is the cap the board
 * always had — `laneCountFor`'s doctrine for the same reason. */
export function columnCapFor(availableHeightPx: number | null): number {
  if (availableHeightPx === null) {
    return DEFAULT_COLUMN_CAP;
  }
  const fits = Math.floor((availableHeightPx - HEADING_ROW - BOTTOM_GUTTER) / CARD_ROW);
  return Math.max(fits, MIN_COLUMN_CAP);
}

/** The narrowest a lane may be before the board drops one, and the gap
 * between lanes. `GAP` is the pixel twin of `--space-6`, the container's own
 * `gap` token: the two must move together, and there is no way to read a token
 * as a number here. `LANE_MIN` is the same 240 the columns' own
 * `flex-basis`/`min-width` has always used. */
const LANE_MIN = 240;
const GAP = 24;

/** How many lanes a container of this width affords, never more than there
 * are columns to fill them. A *capacity*, not a count of what gets drawn:
 * `packLanes` returns fewer whenever the weights do not reach that far.
 *
 * `null` means *unmeasured* — the first layout pass before the observer has
 * run, and every jsdom test, which cannot lay out at all. The capacity there
 * is one lane per column, and `frontierLanes` turns that into the pre-lanes
 * layout outright rather than routing it through the packer: each column
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
  return Math.min(lanesAfforded(widthPx), columnCount);
}

/** Lanes this width has room for, before the columns get a say — the same
 * arithmetic `laneCountFor` caps. Kept apart because the difference between
 * the two is exactly the board's spare width: three lanes afforded, two
 * drawn, and the third is the room a column can continue into. */
function lanesAfforded(widthPx: number): number {
  // `+ GAP` on both sides because n lanes cost n-1 gaps: (w + gap) / (min +
  // gap) is the largest n satisfying n*min + (n-1)*gap <= w.
  return Math.max(Math.floor((widthPx + GAP) / (LANE_MIN + GAP)), 1);
}

/** Which columns land in which lane, as indices into `weights`.
 *
 * Sequential fill, in the given order: every lane aims at the same share —
 * `total / laneCount`, or the tallest single column if that is more — and each
 * column joins the open lane unless it would carry that lane past the share,
 * in which case it opens the next one. Order is never rearranged, so a lane
 * reads top-down in exactly the order `group_frontier` handed over, and the
 * lanes left to right in that same order.
 *
 * Both halves of the share are load-bearing, and each was a visible bug
 * without the other. `total / laneCount` on its own let the first lane of the
 * context axis run to 13 against a 10.3 share and never open the third. The
 * tallest-column floor on its own — paired with a rule that merely aimed
 * *closest* to the share — put the whole urgency board in one lane the moment
 * `calm` was expanded: at 31 rows against three slight bands, swallowing them
 * landed nearer the 31 share than stopping at 6 did.
 *
 * **The returned lanes are all non-empty**, and there may be fewer than
 * `laneCount` of them: a board that affords three lanes but holds one heavy
 * column and three slight ones draws two, and they widen to fill the width
 * the third would have taken. An empty lane is not free — it is a flex item
 * with a `240px` basis, so drawing one is how the board strands whitespace.
 *
 * `weights` are rendered rows, not item counts: what costs vertical space is
 * what is on screen, so a collapsed column weighs its header alone. The caller
 * computes them, because only it knows what it is about to draw. */
export function packLanes(weights: readonly number[], laneCount: number): number[][] {
  const count = Math.min(Math.max(laneCount, 0), weights.length);
  if (count === 0) {
    return [];
  }
  // The share each lane aims at. `total / count` is the even split, but the
  // TALLEST column is a floor under it: that one column is drawn whole in some
  // lane, so the board is at least that tall whatever the packing does, and a
  // lane aiming lower than that spends width to buy height it cannot have.
  // Urgency is exactly that board — three bands of one card in front of a
  // capped `calm` — and without the floor the even split pulled `soon` out
  // into a lane of its own, splitting the severity stack for nothing.
  const total = weights.reduce((sum, weight) => sum + weight, 0);
  const target = Math.max(total / count, ...weights);
  const lanes: number[][] = [[]];
  let carried = 0;
  weights.forEach((weight, index) => {
    // Decided before placing, not after: a column that would carry the open
    // lane past its share finishes that lane instead of joining it. An open
    // lane still holding nothing always takes the column whatever it weighs —
    // a lane that skipped its first column would be drawn empty, or not drawn
    // at all — which is also why the share can never be below the tallest
    // column: the lane holding that one is over its share the moment it does.
    if (carried > 0 && carried + weight > target && lanes.length < count) {
      lanes.push([]);
      carried = 0;
    }
    lanes[lanes.length - 1].push(index);
    carried += weight;
  });
  return lanes;
}

/** Which lanes the board draws, and whether one column may run on into the
 * width they do not use.
 *
 * `lanes` is the packing: whole columns, in reading order, one entry per drawn
 * lane. The unmeasured case is answered here rather than inside the packer,
 * because "one lane per column" is a statement about a runtime that cannot lay
 * out — see `laneCountFor` — and not a packing anyone would choose.
 *
 * `spare` names the room left over. `packLanes` draws only the lanes the
 * weights reached, so a board affording three and drawing two has a lane's
 * width going begging; on the urgency axis that is permanent, because three
 * bands of one card can never fill a lane between them. The column in the last
 * drawn lane may run on into it.
 *
 * **Only that column, and only when it is alone in its lane.** A continuation
 * has to sit immediately right of what it continues or it is not readable as
 * one, and that is the only column for which the spare lanes are adjacent. A
 * last lane holding a stack of columns has no single subject to continue, so
 * the board draws the lanes it filled and leaves the rest, exactly as before.
 *
 * How much of that room is worth taking is the caller's call, not this
 * module's: it turns on how many items the column actually holds against the
 * cap, and this module never inspects an item. */
export type BoardLanes = {
  lanes: number[][];
  spare: { column: number; lanes: number } | null;
};

export function frontierLanes(weights: readonly number[], widthPx: number | null): BoardLanes {
  if (widthPx === null) {
    return { lanes: weights.map((_, index) => [index]), spare: null };
  }
  const lanes = packLanes(weights, laneCountFor(widthPx, weights.length));
  const last = lanes[lanes.length - 1];
  const room = lanesAfforded(widthPx) - lanes.length;
  return {
    lanes,
    spare: room > 0 && last?.length === 1 ? { column: last[0], lanes: room } : null,
  };
}
