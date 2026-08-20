package net.twinion.hummingbird.ui.panes

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswer
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswerState
import uniffi.hummingbird_ffi_mobile.MobilePaneBand

// The web `collapse.test.ts`, ported case for case — the band-stamped
// override semantics are a recorded product decision (ADR-0015/#245), and
// the two clients must read a collapse identically.
class PaneCollapseTest {

    private fun answer(
        band: MobilePaneBand,
        state: MobilePaneAnswerState = MobilePaneAnswerState.ANSWERED,
    ) = MobilePaneAnswer(answerState = state, band = band, withinBand = null)

    @Test
    fun `collapses a dormant pane and opens every livelier band`() {
        assertTrue(PaneCollapse.defaultCollapsed(answer(MobilePaneBand.DORMANT)))
        for (band in listOf(
            MobilePaneBand.LIVE,
            MobilePaneBand.IMMINENT,
            MobilePaneBand.NEAR,
            MobilePaneBand.DISTANT,
        )) {
            assertFalse(PaneCollapse.defaultCollapsed(answer(band)))
        }
    }

    @Test
    fun `collapses a gap and an unbound question whatever their band`() {
        assertTrue(
            PaneCollapse.defaultCollapsed(
                answer(MobilePaneBand.LIVE, MobilePaneAnswerState.UNBOUND),
            ),
        )
        assertTrue(
            PaneCollapse.defaultCollapsed(
                answer(MobilePaneBand.IMMINENT, MobilePaneAnswerState.BOUND_BUT_UNACQUIRED),
            ),
        )
    }

    @Test
    fun `applies an override in either direction while the band still matches`() {
        val opened = mapOf("waste:waste" to CollapseOverride(MobilePaneBand.DORMANT, false))
        assertFalse(PaneCollapse.resolve(opened, "waste:waste", answer(MobilePaneBand.DORMANT)))

        val shut = mapOf("waste:waste" to CollapseOverride(MobilePaneBand.IMMINENT, true))
        assertTrue(PaneCollapse.resolve(shut, "waste:waste", answer(MobilePaneBand.IMMINENT)))
    }

    @Test
    fun `stops applying once the pane's band moves, and falls back to the default`() {
        val stored = mapOf("waste:waste" to CollapseOverride(MobilePaneBand.DORMANT, true))
        assertFalse(PaneCollapse.resolve(stored, "waste:waste", answer(MobilePaneBand.IMMINENT)))
    }

    @Test
    fun `resurrects the override when the band comes back`() {
        val stored = mapOf("waste:waste" to CollapseOverride(MobilePaneBand.DORMANT, false))
        assertFalse(PaneCollapse.resolve(stored, "waste:waste", answer(MobilePaneBand.IMMINENT)))
        // Back to dormant: the opened override applies again — the mismatch
        // was a read-time non-match, never a delete.
        assertFalse(PaneCollapse.resolve(stored, "waste:waste", answer(MobilePaneBand.DORMANT)))
        assertTrue(PaneCollapse.resolve(stored, "race:f1", answer(MobilePaneBand.DORMANT)))
    }

    @Test
    fun `uses the default for a pane nobody has ever overridden`() {
        assertTrue(PaneCollapse.resolve(emptyMap(), "kimi:kimi", answer(MobilePaneBand.DORMANT)))
        assertFalse(PaneCollapse.resolve(emptyMap(), "kimi:kimi", answer(MobilePaneBand.LIVE)))
    }

    @Test
    fun `write round-trips through the stored string form`() {
        val written = PaneCollapse.write(
            emptyMap(),
            "waste:waste",
            CollapseOverride(MobilePaneBand.DORMANT, false),
            listOf("waste:waste", "race:f1"),
        )
        val reread = PaneCollapse.decode(PaneCollapse.encode(written))
        assertEquals(written, reread)
    }

    @Test
    fun `prunes entries for panes that are no longer ranked, and keeps band-mismatched ones`() {
        val current = mapOf(
            "waste:waste" to CollapseOverride(MobilePaneBand.DORMANT, true),
            "gone:gone" to CollapseOverride(MobilePaneBand.NEAR, true),
        )
        val next = PaneCollapse.write(
            current,
            "race:f1",
            CollapseOverride(MobilePaneBand.DISTANT, true),
            listOf("waste:waste", "race:f1"),
        )
        assertEquals(setOf("waste:waste", "race:f1"), next.keys)
        assertEquals(CollapseOverride(MobilePaneBand.DORMANT, true), next["waste:waste"])
    }

    @Test
    fun `reads anything unusable as an empty map rather than failing`() {
        assertEquals(emptyMap<String, CollapseOverride>(), PaneCollapse.decode(null))
        assertEquals(emptyMap<String, CollapseOverride>(), PaneCollapse.decode(""))
        assertEquals(emptyMap<String, CollapseOverride>(), PaneCollapse.decode("not a map at all"))
    }

    @Test
    fun `keeps the readable entries beside an unreadable one`() {
        val good = PaneCollapse.encode(
            mapOf("waste:waste" to CollapseOverride(MobilePaneBand.DORMANT, true)),
        )
        val mixed = good + "\n" + "race:f1\u001FNOT_A_BAND\u001F1" + "\n" + "junk line"
        val read = PaneCollapse.decode(mixed)
        assertEquals(
            mapOf("waste:waste" to CollapseOverride(MobilePaneBand.DORMANT, true)),
            read,
        )
    }
}
