package net.twinion.hummingbird.ui.panes

import org.junit.Assert.assertEquals
import org.junit.Test
import java.time.ZoneId
import uniffi.hummingbird_ffi_mobile.MobileKimiGap
import uniffi.hummingbird_ffi_mobile.MobilePaneFreshness
import uniffi.hummingbird_ffi_mobile.MobileProbeGap
import uniffi.hummingbird_ffi_mobile.MobileRaceGap
import uniffi.hummingbird_ffi_mobile.MobileScpsEvent
import uniffi.hummingbird_ffi_mobile.MobileScpsKind
import uniffi.hummingbird_ffi_mobile.MobileScpsQuestFact
import uniffi.hummingbird_ffi_mobile.MobileWasteGap
import uniffi.hummingbird_ffi_mobile.MobileWorkflowGap

// The expanded cards' load-bearing sentences, pinned the way
// `PaneAnswersTest` pins the collapsed ones — against the web originals'
// wording (`client/web/src/screens/<q>-pane/`), so the two clients explain
// the same gap in the same words.
class PaneExpandedWordsTest {

    @Test
    fun `gap reasons read as the web's own sentences`() {
        assertEquals("No balance has been fetched yet.", kimiGapReason(MobileKimiGap.NotFetched))
        assertEquals(
            "This device doesn't know how to read kimi/v9 yet. Update the app.",
            kimiGapReason(MobileKimiGap.UnknownSchema("kimi/v9")),
        )
        assertEquals(
            "The workflow payload couldn't be read: truncated",
            githubGapReason(MobileWorkflowGap.Malformed("truncated")),
        )
        assertEquals(
            "The probe payload's observation can't be read.",
            uptimeGapReason(MobileProbeGap.ObservationUnreadable),
        )
    }

    @Test
    fun `the stale caveat names its hours, or says it has none to name`() {
        assertEquals("stale — as of 27h ago", staleWords(MobilePaneFreshness.Age(27 * 3_600_000L, null)))
        assertEquals("stale — age unknown", staleWords(MobilePaneFreshness.Unknown))
    }

    @Test
    fun `the github last-run line survives the mirror's optional event`() {
        // The web assumes lastRunEvent; the mobile mirror carries an Option,
        // and an absent event drops its parenthetical, never printing null.
        assertEquals("never run", githubLastRunWords(null, null, null, 1_000L))
        assertEquals(
            "last run success (schedule), 2h ago",
            githubLastRunWords(0L, "success", "schedule", 2 * 3_600_000L),
        )
        assertEquals(
            "last run in progress, 2h ago",
            githubLastRunWords(0L, null, null, 2 * 3_600_000L),
        )
    }

    @Test
    fun `the uptime observation line survives the mirror's optional status`() {
        assertEquals("unreachable — timeout", uptimeObservationWords("timeout", null, 200L))
        assertEquals("answered 503 (wanted 200)", uptimeObservationWords(null, 503L, 200L))
        assertEquals("no status recorded (wanted 200)", uptimeObservationWords(null, null, 200L))
    }

    @Test
    fun `the waste expanded headline is the web's wordier sentence, capitals included`() {
        assertEquals("Trash Today", wasteExpandedHeadline(0, "Tuesday", holiday = false))
        assertEquals("Trash Tonight", wasteExpandedHeadline(1, "Tuesday", holiday = false))
        // A holiday names its day even tomorrow — the moved day IS the answer.
        assertEquals("Trash Wednesday", wasteExpandedHeadline(1, "Wednesday", holiday = true))
        assertEquals("Trash Friday", wasteExpandedHeadline(4, "Friday", holiday = false))
    }

    @Test
    fun `waste and race gaps read as the web's own sentences`() {
        assertEquals(
            "The collection payload names an unknown time zone (Mars/Olympus).",
            wasteGapReason(MobileWasteGap.UnresolvableZone("Mars/Olympus")),
        )
        assertEquals(
            "The collection schedule is out of date: it still names Monday 2026-08-17, " +
                "which has passed.",
            wasteGapReason(MobileWasteGap.PastCollection("2026-08-17", 1u)),
        )
        assertEquals(
            "No schedule has been fetched for this series yet.",
            raceGapReason(MobileRaceGap.NotFetched),
        )
    }

    @Test
    fun `the race day label and clock read device-local, no zone suffix`() {
        val zone = ZoneId.of("America/Los_Angeles")
        // 2026-08-20 06:00 PT
        val nowMs = 1_787_230_800_000L
        assertEquals("Today", raceDayLabel(nowMs + 2 * 3_600_000L, nowMs, zone))
        assertEquals("Tomorrow", raceDayLabel(nowMs + 26 * 3_600_000L, nowMs, zone))
        assertEquals("Sunday", raceDayLabel(nowMs + 3 * 86_400_000L, nowMs, zone))
        assertEquals("Sep 20", raceDayLabel(nowMs + 31 * 86_400_000L, nowMs, zone))
        assertEquals("8:00 AM", raceClock(nowMs + 2 * 3_600_000L, zone))
    }

    // -------------------------------------------------------------- scps

    private val zone = ZoneId.of("UTC")

    private fun scpsEvent(
        kind: MobileScpsKind = MobileScpsKind.MEETING,
        topic: String? = "Impressions of Venice",
        startMs: Long = 0L,
        startDate: String = "2026-03-01",
        location: String? = null,
        notes: String? = null,
        daysUntil: Long = 0L,
        inProgress: Boolean = false,
    ): MobileScpsEvent = MobileScpsEvent(
        id = "e1",
        kind = kind,
        topic = topic,
        startMs = startMs,
        endMs = startMs + 3_600_000L,
        startDate = startDate,
        location = location,
        notes = notes,
        daysUntil = daysUntil,
        inProgress = inProgress,
    )

    @Test
    fun `scpsTimeLabel formats 2-00pm and 9-00am plainly, no space, lowercase`() {
        // 2026-03-01 14:00 and 09:00 UTC.
        assertEquals("2:00pm", scpsTimeLabel(1_772_373_600_000L, zone))
        assertEquals("9:00am", scpsTimeLabel(1_772_355_600_000L, zone))
    }

    @Test
    fun `scpsMonthName reads the two-digit month token`() {
        assertEquals("September", scpsMonthName("2026-09"))
        assertEquals("January", scpsMonthName("2026-01"))
    }

    @Test
    fun `scpsDayLabel reads today, tomorrow, weekday and date off daysUntil and startDate`() {
        assertEquals("today", scpsDayLabel(scpsEvent(daysUntil = 0L)))
        assertEquals("today", scpsDayLabel(scpsEvent(daysUntil = 3L, inProgress = true)))
        assertEquals("tomorrow", scpsDayLabel(scpsEvent(daysUntil = 1L)))
        // 2026-03-06 is a Friday, 5 days out.
        assertEquals("Fri", scpsDayLabel(scpsEvent(daysUntil = 5L, startDate = "2026-03-06")))
        assertEquals("20 Mar", scpsDayLabel(scpsEvent(daysUntil = 19L, startDate = "2026-03-20")))
    }

    @Test
    fun `scpsCardTitle titles with kind and topic only — no day or time`() {
        assertEquals(
            "SCPS Meeting — Impressions of Venice",
            scpsCardTitle(scpsEvent(kind = MobileScpsKind.MEETING, topic = "Impressions of Venice")),
        )
        assertEquals(
            "SCPS Happy Hour in progress",
            scpsCardTitle(scpsEvent(kind = MobileScpsKind.HAPPY_HOUR, topic = null, inProgress = true)),
        )
        assertEquals(
            "SCPS Happy Hour",
            scpsCardTitle(scpsEvent(kind = MobileScpsKind.HAPPY_HOUR, topic = null, inProgress = false)),
        )
        assertEquals(
            "SCPS event",
            scpsCardTitle(scpsEvent(kind = MobileScpsKind.EVENT, topic = null)),
        )
    }

    @Test
    fun `scpsQuestLine shows the current phrase, the last-posted one, or unset`() {
        assertEquals(
            "Photo Quest — Reflected Light",
            scpsQuestLine(MobileScpsQuestFact.Current("Reflected Light"), 0L, zone),
        )
        // 2026-10-01 UTC.
        val octoberFirst = 1_790_812_800_000L
        assertEquals(
            "No quest posted for October; last: Reflected Light (September)",
            scpsQuestLine(MobileScpsQuestFact.Other("2026-09", "Reflected Light"), octoberFirst, zone),
        )
        assertEquals("No quest set", scpsQuestLine(MobileScpsQuestFact.None, 0L, zone))
    }
}
