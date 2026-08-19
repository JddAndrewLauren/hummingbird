package net.twinion.hummingbird

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswer
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswerState
import uniffi.hummingbird_ffi_mobile.MobilePaneBand
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileStandingQuestion

// `StatusViewModel`'s control flow, with a fake `rankPanesFn` only — the
// same house shape `RulesViewModel`'s own doc names. This class decides
// nothing about a pane, so there is nothing here to pin beyond "the state
// it renders is exactly what the seam handed back".
class StatusViewModelTest {

    private fun pane(question: MobileStandingQuestion, band: MobilePaneBand) = MobileRankedPane(
        standingQuestion = question,
        subjectKey = "the-subject",
        paneKey = "${question.name.lowercase()}:the-subject",
        answer = MobilePaneAnswer(
            answerState = MobilePaneAnswerState.ANSWERED,
            band = band,
            withinBand = null,
        ),
    )

    @Test
    fun `starts loading and moves to the seams own ranked list`() = runBlocking {
        val panes = listOf(
            pane(MobileStandingQuestion.KIMI, MobilePaneBand.DORMANT),
            pane(MobileStandingQuestion.REACHABILITY, MobilePaneBand.LIVE),
        )
        val vm = StatusViewModel(rankPanesFn = { panes })

        assertEquals(StatusState.Loading, vm.state.value)

        vm.load(1_000L)

        assertEquals(StatusState.Loaded(panes), vm.state.value)
    }

    @Test
    fun `a reload replaces the previous panes rather than appending`() = runBlocking {
        var call = 0
        val vm = StatusViewModel(
            rankPanesFn = {
                call += 1
                if (call == 1) listOf(pane(MobileStandingQuestion.KIMI, MobilePaneBand.DORMANT))
                else listOf(pane(MobileStandingQuestion.KIMI, MobilePaneBand.LIVE))
            },
        )

        vm.load(1_000L)
        vm.load(2_000L)

        val loaded = vm.state.value as StatusState.Loaded
        assertEquals(1, loaded.panes.size)
        assertTrue(loaded.panes.single().answer.band == MobilePaneBand.LIVE)
    }
}
