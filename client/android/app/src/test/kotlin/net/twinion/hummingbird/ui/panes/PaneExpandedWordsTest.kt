package net.twinion.hummingbird.ui.panes

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileKimiGap
import uniffi.hummingbird_ffi_mobile.MobilePaneFreshness
import uniffi.hummingbird_ffi_mobile.MobileProbeGap
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
}
