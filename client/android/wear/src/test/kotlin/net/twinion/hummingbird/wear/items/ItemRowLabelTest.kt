package net.twinion.hummingbird.wear.items

import net.twinion.hummingbird.ui.theme.Ember400
import net.twinion.hummingbird.ui.theme.Ink400
import net.twinion.hummingbird.ui.theme.UrgencyOverdueDark
import net.twinion.hummingbird.ui.theme.UrgencySoonDark
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileUrgencyBand

// The Items row's words: a rendering of the band the core decided and the
// deadline string, against the board's own day. 2026-09-10 is a Thursday.
class ItemRowLabelTest {

    private val today = "2026-09-10"

    @Test
    fun `overdue names the day it was due`() {
        assertEquals("OVERDUE · TUE", urgencyRowLabel(MobileUrgencyBand.OVERDUE, "2026-09-08", today))
        assertEquals("OVERDUE · MON", urgencyRowLabel(MobileUrgencyBand.OVERDUE, "2026-09-07T16:00", today))
        assertEquals("OVERDUE", urgencyRowLabel(MobileUrgencyBand.OVERDUE, "next tuesday", today))
    }

    @Test
    fun `due within the day is today, or the weekday when the core's window reaches tomorrow`() {
        assertEquals("DUE TODAY", urgencyRowLabel(MobileUrgencyBand.NOW, "2026-09-10", today))
        assertEquals("DUE TODAY", urgencyRowLabel(MobileUrgencyBand.NOW, "2026-09-10T18:00", today))
        assertEquals("DUE FRI", urgencyRowLabel(MobileUrgencyBand.NOW, "2026-09-11T09:00", today))
    }

    @Test
    fun `soon names the weekday, calm the date or its absence`() {
        assertEquals("DUE SAT", urgencyRowLabel(MobileUrgencyBand.SOON, "2026-09-12", today))
        assertEquals("DUE OCT 3", urgencyRowLabel(MobileUrgencyBand.CALM, "2026-10-03", today))
        assertEquals("NO DEADLINE", urgencyRowLabel(MobileUrgencyBand.CALM, null, today))
    }

    @Test
    fun `the dot is the dark urgency colour, calm included`() {
        assertEquals(UrgencyOverdueDark, urgencyDot(MobileUrgencyBand.OVERDUE))
        assertEquals(Ember400, urgencyDot(MobileUrgencyBand.NOW))
        assertEquals(UrgencySoonDark, urgencyDot(MobileUrgencyBand.SOON))
        assertEquals(Ink400, urgencyDot(MobileUrgencyBand.CALM))
    }

    @Test
    fun `the meta line is the context then the size, and absent when both are`() {
        assertEquals("@PHONE · SIZE:QUICK", itemMetaLine("@phone", "quick"))
        assertEquals("@PHONE", itemMetaLine("@phone", null))
        assertEquals("SIZE:DEEP", itemMetaLine("", "deep"))
        assertNull(itemMetaLine(null, null))
    }
}
