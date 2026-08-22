package net.twinion.hummingbird

// How Now's frontier columns are distributed across the wide board's width —
// a verbatim Kotlin port of `client/web/src/screens/frontier-lanes.ts`
// (`laneCountFor`/`packLanes`), which is the parity source: any change lands
// there first and is ported here, and `FrontierLanesTest` pins the two
// files' constants against each other so they cannot drift apart.
//
// Client-side under ADR-0025's carve-out, like its web twin: the decision
// consumes a *measured width* — a fact only the rendering runtime has —
// and never inspects an item. `hummingbird_core::decisions::frontier::
// group_frontier` still owns everything about the columns themselves:
// membership, order, labels, the no-value bucket. This module takes a count
// of columns, a weight per column and a width, and answers where each one
// is drawn.

/** The narrowest a lane may be before the board drops one, and the gap
 * between lanes — the web's `LANE_MIN`/`GAP`, dp-for-px. */
internal const val LANE_MIN_DP = 240
internal const val LANE_GAP_DP = 24

/** How many lanes a container of this width affords, never more than there
 * are columns to fill them.
 *
 * `null` means *unmeasured* — the first frame before `BoxWithConstraints`
 * has bounded the width. The answer there is one lane per column, exactly
 * the web's own pre-observer answer: each column alone in its own lane, in
 * `group_frontier`'s order. */
internal fun laneCountFor(widthDp: Int?, columnCount: Int): Int {
    if (columnCount <= 0) return 0
    if (widthDp == null) return columnCount
    // `+ GAP` on both sides because n lanes cost n-1 gaps: (w + gap) /
    // (min + gap) is the largest n satisfying n*min + (n-1)*gap <= w.
    val fits = (widthDp + LANE_GAP_DP) / (LANE_MIN_DP + LANE_GAP_DP)
    return fits.coerceAtLeast(1).coerceAtMost(columnCount)
}

/** Which columns land in which lane, as indices into [weights].
 *
 * Greedy, in the given order: each column goes to the lane with the least
 * in it so far, leftmost on a tie. Two properties follow, and both are the
 * point: the first `laneCount` columns fan across the top exactly as the
 * ordering put them — every lane starts empty, so each takes the next one —
 * and a short column then stacks under whichever lane is currently shortest
 * instead of opening a track of its own.
 *
 * [weights] are rendered rows, not item counts: what costs vertical space
 * is what is on screen, so a collapsed column weighs its header alone. The
 * caller computes them, because only it knows what it is about to draw. */
internal fun packLanes(weights: List<Int>, laneCount: Int): List<List<Int>> {
    if (laneCount <= 0) return emptyList()
    val lanes = List(laneCount) { mutableListOf<Int>() }
    val totals = IntArray(laneCount)
    weights.forEachIndexed { index, weight ->
        var pick = 0
        for (lane in 1 until laneCount) {
            // Strictly less, so an equal total leaves the leftmost lane the
            // winner.
            if (totals[lane] < totals[pick]) pick = lane
        }
        lanes[pick].add(index)
        totals[pick] += weight
    }
    return lanes
}
