package net.twinion.hummingbird.core

import java.time.ZoneId
import java.time.ZonedDateTime
import org.junit.Assert.assertEquals
import org.junit.Test

/** `WallClock.todayDeadline` is the port of `capture-meta.ts`'s
 * `todayDeadline`, and this is the port of its test: the local-midnight
 * pair, in a zone west of Greenwich, where a UTC reading would name the
 * wrong day for the whole evening. */
class WallClockTodayDeadlineTest {

    private val la: ZoneId = ZoneId.of("America/Los_Angeles")

    private fun at(year: Int, month: Int, day: Int, hour: Int, minute: Int): Long =
        ZonedDateTime.of(year, month, day, hour, minute, 0, 0, la).toInstant().toEpochMilli()

    @Test
    fun `the device's own calendar date, date only`() {
        assertEquals("2026-09-03", WallClock.todayDeadline(at(2026, 9, 3, 14, 30), la))
    }

    @Test
    fun `either side of local midnight is the local day, not the UTC one`() {
        // 23:30 in Los Angeles is already the next day in UTC.
        assertEquals("2026-09-03", WallClock.todayDeadline(at(2026, 9, 3, 23, 30), la))
        assertEquals("2026-09-04", WallClock.todayDeadline(at(2026, 9, 4, 0, 30), la))
    }

    @Test
    fun `it is local's date half, so the two can never disagree`() {
        val ms = at(2026, 12, 31, 23, 59)
        assertEquals(WallClock.local(ms, la).take(10), WallClock.todayDeadline(ms, la))
    }
}
