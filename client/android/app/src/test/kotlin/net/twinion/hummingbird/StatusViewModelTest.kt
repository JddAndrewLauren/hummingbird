package net.twinion.hummingbird

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswer
import uniffi.hummingbird_ffi_mobile.MobileKimiGap
import uniffi.hummingbird_ffi_mobile.MobileKimiResolved
import uniffi.hummingbird_ffi_mobile.MobilePaneFacts
import uniffi.hummingbird_ffi_mobile.MobileProbeGap
import uniffi.hummingbird_ffi_mobile.MobileProbeResolved
import uniffi.hummingbird_ffi_mobile.MobileRaceGap
import uniffi.hummingbird_ffi_mobile.MobileRaceResolved
import uniffi.hummingbird_ffi_mobile.MobileWasteGap
import uniffi.hummingbird_ffi_mobile.MobileWasteResolved
import uniffi.hummingbird_ffi_mobile.MobileWeekendGap
import uniffi.hummingbird_ffi_mobile.MobileWeekendResolved
import uniffi.hummingbird_ffi_mobile.MobileWorkflowGap
import uniffi.hummingbird_ffi_mobile.MobileWorkflowResolved
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
        facts = paneFacts(question),
    )

    /** The simplest honest facts arm for [question] — the fresh-device gap
     * (or the two genuinely-optional `null`s), so a fixture can never pair
     * a question with a foreign arm. Exhaustive, no `else`: a ninth
     * question breaks this fixture loudly. */
    private fun paneFacts(question: MobileStandingQuestion): MobilePaneFacts = when (question) {
        MobileStandingQuestion.WASTE ->
            MobilePaneFacts.Waste(resolved = MobileWasteResolved.Gap(gap = MobileWasteGap.NotFetched))
        MobileStandingQuestion.WEEKEND ->
            MobilePaneFacts.Weekend(resolved = MobileWeekendResolved.Gap(gap = MobileWeekendGap.NOT_CONNECTED))
        MobileStandingQuestion.VACATION -> MobilePaneFacts.Vacation(resolved = null)
        MobileStandingQuestion.RACE ->
            MobilePaneFacts.Race(resolved = MobileRaceResolved.Gap(gap = MobileRaceGap.NotFetched))
        MobileStandingQuestion.KIMI ->
            MobilePaneFacts.Kimi(resolved = MobileKimiResolved.Gap(gap = MobileKimiGap.NotFetched))
        MobileStandingQuestion.GITHUB ->
            MobilePaneFacts.Github(resolved = MobileWorkflowResolved.Gap(gap = MobileWorkflowGap.NotFetched))
        MobileStandingQuestion.UPTIME ->
            MobilePaneFacts.Uptime(resolved = MobileProbeResolved.Gap(gap = MobileProbeGap.NotFetched))
        MobileStandingQuestion.REACHABILITY -> MobilePaneFacts.Reachability(facts = null)
    }

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
