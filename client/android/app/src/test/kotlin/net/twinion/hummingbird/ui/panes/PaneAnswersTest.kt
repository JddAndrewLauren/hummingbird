package net.twinion.hummingbird.ui.panes

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobilePaneBand
import uniffi.hummingbird_ffi_mobile.MobileProbeBody
import uniffi.hummingbird_ffi_mobile.MobileProbeExpected
import uniffi.hummingbird_ffi_mobile.MobileTrip
import uniffi.hummingbird_ffi_mobile.MobileTripPhase
import uniffi.hummingbird_ffi_mobile.MobileWeekendCounts

// The ported headline words, pinned against their sources: each fixture
// here is the web renderer's own test vocabulary (`waste.ts`, `kimi.ts`,
// `uptime.ts`, `weekend.ts`, `vacation.ts`, `race.ts`), and `relativeAge`'s
// fixtures are `hummingbird-core::decisions::settings`'s Rust test values
// verbatim — the wording is per-client by ADR-0025, so the PIN is what
// stops the two clients' sentences drifting apart silently.
class PaneAnswersTest {

    @Test
    fun `relativeAge matches the Rust original's own fixtures`() {
        assertEquals("just now", relativeAge(0))
        assertEquals("just now", relativeAge(59_999))
        assertEquals("1m ago", relativeAge(60_000))
        assertEquals("59m ago", relativeAge(59 * 60_000))
        assertEquals("1h ago", relativeAge(60 * 60_000))
        assertEquals("23h ago", relativeAge(23 * 60 * 60_000))
        assertEquals("1d ago", relativeAge(24 * 60 * 60_000))
    }

    @Test
    fun `waste reads today, tonight, then the weekday — and a holiday names its day even tomorrow`() {
        assertEquals("Trash today", wasteCollapsedHeadline(0, "Tuesday", holiday = false))
        assertEquals("Trash tonight", wasteCollapsedHeadline(1, "Tuesday", holiday = false))
        // The holiday week names the actual day — "tonight" would hide the
        // very thing that changed.
        assertEquals("Tuesday · 1d", wasteCollapsedHeadline(1, "Tuesday", holiday = true))
        assertEquals("Friday · 4d", wasteCollapsedHeadline(4, "Friday", holiday = false))
    }

    @Test
    fun `kimi formats the sign before the symbol and reads the decided band's word`() {
        assertEquals("-$1.00", formatUsd(-1.0))
        assertEquals("$12.34", formatUsd(12.34))
        assertEquals("$0.00 — exhausted", kimiCollapsedHeadline(0.0, MobilePaneBand.LIVE))
        assertEquals("$1.50 — critical", kimiCollapsedHeadline(1.5, MobilePaneBand.IMMINENT))
        assertEquals("$4.00 — running low", kimiCollapsedHeadline(4.0, MobilePaneBand.NEAR))
        assertEquals("$40.00 left", kimiCollapsedHeadline(40.0, MobilePaneBand.DORMANT))
    }

    @Test
    fun `uptime reads the body only — off-as-expected, unreachable, divergent, healthy`() {
        fun body(
            expected: MobileProbeExpected,
            observed: Long?,
            error: String?,
        ) = MobileProbeBody(
            expected = expected,
            expectStatus = 200,
            observedStatus = observed,
            error = error,
        )
        assertEquals(
            "hb · off, as expected",
            uptimeCollapsedHeadline("hb", body(MobileProbeExpected.OFF, null, "refused")),
        )
        assertEquals(
            "hb · reachable — expected off",
            uptimeCollapsedHeadline("hb", body(MobileProbeExpected.OFF, 200, null)),
        )
        assertEquals(
            "hb · unreachable — timeout",
            uptimeCollapsedHeadline("hb", body(MobileProbeExpected.ON, null, "timeout")),
        )
        assertEquals(
            "hb · unexpected status 503 (wanted 200)",
            uptimeCollapsedHeadline("hb", body(MobileProbeExpected.ON, 503, null)),
        )
        assertEquals(
            "hb · 200 as expected",
            uptimeCollapsedHeadline("hb", body(MobileProbeExpected.ON, 200, null)),
        )
    }

    @Test
    fun `weekend counts speak in the web's own joined parts, and an empty window is honest about under way`() {
        assertEquals(
            "Nothing planned",
            weekendCollapsedHeadline(MobileWeekendCounts(0, 0, 0), underWay = false),
        )
        assertEquals(
            "Clear so far",
            weekendCollapsedHeadline(MobileWeekendCounts(0, 0, 0), underWay = true),
        )
        assertEquals(
            "2 due · 1 on the calendar · 3 planned",
            weekendCollapsedHeadline(MobileWeekendCounts(events = 1, due = 2, scheduled = 3), underWay = false),
        )
    }

    @Test
    fun `vacation phases read as the web's own sentences`() {
        fun trip(phase: MobileTripPhase, daysUntil: Long = 3) = MobileTrip(
            id = "t",
            location = "Lisbon",
            startDate = "2026-09-01",
            lastDate = "2026-09-08",
            startMs = 0,
            endMs = 0,
            phase = phase,
            daysUntil = daysUntil,
            lengthDays = 8,
            dayOfTrip = 2,
        )
        assertEquals("Nothing booked in the next 6 months", vacationTripHeadline(null))
        assertEquals("Lisbon in 3 days", vacationTripHeadline(trip(MobileTripPhase.UPCOMING)))
        assertEquals("Lisbon tomorrow", vacationTripHeadline(trip(MobileTripPhase.UPCOMING, daysUntil = 1)))
        assertEquals("Lisbon today", vacationTripHeadline(trip(MobileTripPhase.DEPARTS_TODAY)))
        assertEquals("In Lisbon · day 2 of 8", vacationTripHeadline(trip(MobileTripPhase.UNDER_WAY)))
        assertEquals("Home today from Lisbon", vacationTripHeadline(trip(MobileTripPhase.RETURNS_TODAY)))
        assertEquals("Lisbon is over", vacationTripHeadline(trip(MobileTripPhase.PAST)))
    }

    @Test
    fun `race countdown and names read as the web's — GP abbreviation included`() {
        assertEquals("Monaco GP", abbreviateEventName("Monaco Grand Prix"))
        assertEquals("Indy 500", abbreviateEventName("Indy 500"))
        assertEquals(Pair("90", "min"), countdown(90 * 60_000L))
        assertEquals(Pair("3", "hr"), countdown(3 * 3_600_000L))
        assertEquals(Pair("3", "days"), countdown(3 * 86_400_000L))
        assertEquals("F1", seriesLabel("f1"))
        assertEquals("IndyCar", seriesLabel("indycar"))
        assertEquals("WEC", seriesLabel("wec"))
    }

    @Test
    fun `reachability chooses its verb by whether the latest attempt landed`() {
        assertEquals(
            "Synced 5m ago",
            reachabilityHeadline(ReachabilityWords(5 * 60_000, stale = false, latestAttemptLanded = true)),
        )
        assertEquals(
            "Last synced 2h ago",
            reachabilityHeadline(ReachabilityWords(2 * 3_600_000, stale = true, latestAttemptLanded = false)),
        )
        assertEquals("Never synced on this device.", reachabilityHeadline(null))
    }
}
