package net.twinion.hummingbird.wear.tile

import java.util.Locale
import net.twinion.hummingbird.wear.syncAgeLine
import uniffi.hummingbird_ffi_mobile.MobileUrgencyBand
import uniffi.hummingbird_ffi_mobile.NowBoardRecord

// What the capture tile knows, and the words and arcs it makes of it (the
// Wear capture design handoff, 2026-09-10, option 1d). Pure: the service
// reads the mirror and the sync history and hands the facts here; nothing
// in this file touches a clock, a core or a resource. Pinned by
// `TileFactsTest`.
//
// Two numbers, not four. The core's urgency has four bands; the tile's arc
// has two segments — overdue in `--urgency-overdue`, and everything else
// with a deadline in reach (`now` and `soon`, the core's 24-hour and 3-day
// windows) in `--urgency-soon` — because on a 1.4-inch face two colours
// read at a glance and three do not (operator decision, 2026-09-10). The
// bands themselves stay the core's: this file counts rows by the band the
// board already put them in, and decides no band of its own.

/** What the tile draws from: the counts (or none), and the sync age the
 * count line and the arc are judged against. */
data class TileFacts(val counts: TileCounts?, val lastInformativeAtMs: Long?, val nowMs: Long)

/** The counts the arc and the count line draw. `null` at the caller means
 * there is nothing to count from — no token yet, or a core that could not
 * be opened — and the tile draws the disc alone. */
data class TileCounts(val overdue: Int, val soon: Int)

/** The board's rows, counted by the band the core gave each. */
fun tileCounts(board: NowBoardRecord): TileCounts {
    var overdue = 0
    var soon = 0
    for (column in board.columns) {
        for (item in column.items) {
            when (item.urgency) {
                MobileUrgencyBand.OVERDUE -> overdue += 1
                MobileUrgencyBand.NOW -> soon += 1
                MobileUrgencyBand.SOON -> soon += 1
                MobileUrgencyBand.CALM -> Unit
            }
        }
    }
    return TileCounts(overdue, soon)
}

/** The mono line over the disc — `2 OVERDUE · 3 SOON`, `NOTHING DUE` — or
 * the honesty line in its place when the mirror is over an hour old
 * (`SYNCED 2H AGO`, the home screen's rule in the tile's register) or has
 * never been filled (`NOT SYNCED YET`): stale counts are never shown as
 * current ones. `null` when there are no counts at all, and the tile says
 * nothing. */
fun tileCountLine(counts: TileCounts?, lastInformativeAtMs: Long?, nowMs: Long): String? {
    if (counts == null) return null
    // The home screen's honesty rule, in this register: one spelling of
    // "an hour is stale", not two.
    syncAgeLine(lastInformativeAtMs, nowMs)?.let { return it.uppercase(Locale.ROOT) }
    val parts = buildList {
        if (counts.overdue > 0) add("${counts.overdue} OVERDUE")
        if (counts.soon > 0) add("${counts.soon} SOON")
    }
    return if (parts.isEmpty()) "NOTHING DUE" else parts.joinToString(" · ")
}

/** The counts the arc may draw: the same ones the count line names, and
 * `null` once the line has yielded to `SYNCED nH AGO` — a stale number
 * hidden behind a current-looking picture of it would be the same lie. */
fun arcCounts(counts: TileCounts?, lastInformativeAtMs: Long?, nowMs: Long): TileCounts? =
    if (counts == null || syncAgeLine(lastInformativeAtMs, nowMs) != null) null else counts

/** The two arc lengths, in degrees: [DEGREES_PER_ITEM] each, overdue first,
 * the pair scaled down together once they would pass [MAX_ARC_DEGREES] so
 * the arc never closes on itself and the proportion between them holds. */
data class ArcDegrees(val overdue: Float, val soon: Float)

fun arcDegrees(counts: TileCounts?): ArcDegrees {
    if (counts == null) return ArcDegrees(0f, 0f)
    val overdue = counts.overdue * DEGREES_PER_ITEM
    val soon = counts.soon * DEGREES_PER_ITEM
    val total = overdue + soon
    if (total <= MAX_ARC_DEGREES) return ArcDegrees(overdue, soon)
    val scale = MAX_ARC_DEGREES / total
    return ArcDegrees(overdue * scale, soon * scale)
}

/** The design's proportion: two overdue items drew 118px of a 842px
 * circumference — fifty degrees, twenty-five each. */
const val DEGREES_PER_ITEM = 25f

/** Where the arc starts, clockwise from twelve: ten o'clock, as drawn. */
const val ARC_START_DEGREES = 300f

/** The most the two segments may span together — short of the start, with
 * the gap between them kept. */
const val MAX_ARC_DEGREES = 250f

/** The gap between the overdue and soon segments, when both are drawn. */
const val ARC_GAP_DEGREES = 6f

