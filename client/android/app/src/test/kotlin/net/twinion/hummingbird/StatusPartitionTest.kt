package net.twinion.hummingbird

import net.twinion.hummingbird.ui.panes.StatusPartition
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswer
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswerState
import uniffi.hummingbird_ffi_mobile.MobilePaneBand

// The quiet stack's split. Pure and exhaustive: every band against every
// answer state, so a change to the rule cannot pass by covering only the
// combinations a fixture happened to build.
class StatusPartitionTest {

    private fun answer(band: MobilePaneBand, state: MobilePaneAnswerState) =
        MobilePaneAnswer(answerState = state, band = band, withinBand = null)

    @Test
    fun `only an answered dormant pane is quiet`() {
        for (band in MobilePaneBand.entries) {
            for (state in MobilePaneAnswerState.entries) {
                val quiet = band == MobilePaneBand.DORMANT &&
                    state == MobilePaneAnswerState.ANSWERED
                assertTrue(
                    "$band + $state should ${if (quiet) "fold into the quiet card" else "announce"}",
                    StatusPartition.isProblem(answer(band, state)) == !quiet,
                )
            }
        }
    }

    @Test
    fun `a gap announces rather than hiding behind a chip`() {
        // A pane nobody has polled has no answer to call "as expected", so
        // folding it in would make the quiet card's "N as expected" a lie.
        assertTrue(
            StatusPartition.isProblem(
                answer(MobilePaneBand.DORMANT, MobilePaneAnswerState.BOUND_BUT_UNACQUIRED),
            ),
        )
    }

    @Test
    fun `an unbound pane announces, which is what keeps its Settings door on screen`() {
        assertTrue(
            StatusPartition.isProblem(
                answer(MobilePaneBand.DORMANT, MobilePaneAnswerState.UNBOUND),
            ),
        )
    }

    @Test
    fun `a healthy pane is the only thing that goes quiet`() {
        assertFalse(
            StatusPartition.isProblem(
                answer(MobilePaneBand.DORMANT, MobilePaneAnswerState.ANSWERED),
            ),
        )
    }
}
