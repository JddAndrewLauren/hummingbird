package net.twinion.hummingbird

// How Now's frontier columns are distributed across the wide board's width —
// a verbatim Kotlin port of `client/web/src/screens/frontier-lanes.ts`
// (`laneCountFor`/`laneWeightsFor`/`packLanes`/`frontierLanes`), which is
// the parity source — its header carries why the packing fills lanes in
// order and weighs resting heights. Any change lands there first and is
// ported here, and `FrontierLanesTest` pins the two files' constants and
// rules against each other so they cannot drift apart.
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

/** What each column costs the packing in rows — the web's
 * `laneWeightsFor`: a heading, the cards it shows at rest, and the "n more"
 * control if it has one.
 *
 * It takes item counts and a cap and nothing else, on purpose: a column's
 * weight is its RESTING height, so no collapse, reveal or selection can
 * repack the board. The web module's doc carries why. */
internal fun laneWeightsFor(itemCounts: List<Int>, columnCap: Int): List<Int> =
    itemCounts.map { count -> 1 + minOf(count, columnCap) + (if (count > columnCap) 1 else 0) }

/** Which columns land in which lane, as indices into [weights] — the web's
 * `packLanes`, whose doc carries the reasoning.
 *
 * Sequential fill, in the given order: every lane aims at `total /
 * laneCount` or the tallest single column, whichever is larger, and a
 * column that would carry a non-empty open lane past that share opens the
 * next lane instead, while lanes remain. Only the lanes reached are
 * returned — never an empty one, and possibly fewer than [laneCount]. */
internal fun packLanes(weights: List<Int>, laneCount: Int): List<List<Int>> {
    val count = minOf(laneCount.coerceAtLeast(0), weights.size)
    if (count == 0) return emptyList()
    val target = maxOf(weights.sum().toDouble() / count, weights.max().toDouble())
    val lanes = mutableListOf(mutableListOf<Int>())
    var carried = 0
    weights.forEachIndexed { index, weight ->
        if (carried > 0 && carried + weight > target && lanes.size < count) {
            lanes.add(mutableListOf())
            carried = 0
        }
        lanes.last().add(index)
        carried += weight
    }
    return lanes
}

/** The lanes the board draws — the web's `frontierLanes` without its
 * `spare` (Android draws no column across lanes). An unmeasured width
 * (null) answers one lane per column here rather than through the packer,
 * which would stack them: that is the pre-lanes layout, not a packing. */
internal fun frontierLanes(weights: List<Int>, widthDp: Int?): List<List<Int>> {
    if (widthDp == null) return weights.indices.map { listOf(it) }
    return packLanes(weights, laneCountFor(widthDp, weights.size))
}
