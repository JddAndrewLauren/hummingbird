package net.twinion.hummingbird.wear.tile

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileUrgencyBand
import uniffi.hummingbird_ffi_mobile.NowBoardRecord
import uniffi.hummingbird_ffi_mobile.NowColumnRecord
import uniffi.hummingbird_ffi_mobile.NowItemRecord

// The tile's counts, words and arc lengths over a fake board: overdue is
// its own number, now and soon are one, calm is not counted; the count line
// yields to the honesty line once the mirror is an hour old; the arcs
// scale together past the cap.
class TileFactsTest {

    private fun item(band: MobileUrgencyBand) = NowItemRecord(
        id = band.name, title = band.name, deadline = null, urgency = band, priority = 0L,
        context = null, size = null, energy = null, availableActions = emptyList(),
        stage = "ready", canMarkDone = true,
    )

    private fun board(vararg bands: MobileUrgencyBand) = NowBoardRecord(
        columns = listOf(NowColumnRecord(value = null, label = null, items = bands.map(::item))),
        blocked = emptyList(), contexts = emptyList(), liveColumnKeys = emptyList(),
        shownCount = 0u, totalCount = 0u,
    )

    private val hour = 60L * 60L * 1000L

    @Test
    fun `overdue is counted alone, now and soon together, calm not at all`() {
        val counts = tileCounts(
            board(
                MobileUrgencyBand.OVERDUE, MobileUrgencyBand.OVERDUE,
                MobileUrgencyBand.NOW, MobileUrgencyBand.SOON, MobileUrgencyBand.SOON,
                MobileUrgencyBand.CALM, MobileUrgencyBand.CALM,
            ),
        )
        assertEquals(TileCounts(overdue = 2, soon = 3), counts)
    }

    @Test
    fun `the count line names what is due, in the design's words`() {
        assertEquals("2 OVERDUE · 3 SOON", tileCountLine(TileCounts(2, 3), 1_000L, 1_000L))
        assertEquals("1 OVERDUE", tileCountLine(TileCounts(1, 0), 1_000L, 1_000L))
        assertEquals("4 SOON", tileCountLine(TileCounts(0, 4), 1_000L, 1_000L))
        assertEquals("NOTHING DUE", tileCountLine(TileCounts(0, 0), 1_000L, 1_000L))
    }

    @Test
    fun `stale counts are never shown as current ones`() {
        assertEquals("NOT SYNCED YET", tileCountLine(TileCounts(2, 3), null, hour))
        assertEquals("2 OVERDUE · 3 SOON", tileCountLine(TileCounts(2, 3), 0L, hour - 1))
        assertEquals("SYNCED 1H AGO", tileCountLine(TileCounts(2, 3), 0L, hour))
        assertEquals("SYNCED 26H AGO", tileCountLine(TileCounts(2, 3), 0L, 26 * hour + 5))
        assertNull(tileCountLine(null, 0L, 0L))
    }

    @Test
    fun `the arcs are twenty-five degrees an item, scaled together past the cap`() {
        assertEquals(ArcDegrees(50f, 75f), arcDegrees(TileCounts(2, 3)))
        assertEquals(ArcDegrees(0f, 0f), arcDegrees(null))
        val capped = arcDegrees(TileCounts(10, 10))
        assertEquals(MAX_ARC_DEGREES, capped.overdue + capped.soon, 0.001f)
        assertEquals(capped.overdue, capped.soon, 0.001f)
    }
}
