package net.twinion.hummingbird.wear.questions

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileHomeworkFacts
import uniffi.hummingbird_ffi_mobile.MobileHomeworkResolved
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswer
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswerState
import uniffi.hummingbird_ffi_mobile.MobilePaneBand
import uniffi.hummingbird_ffi_mobile.MobilePaneFacts
import uniffi.hummingbird_ffi_mobile.MobileQuestionRosterEntry
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileStandingQuestion
import uniffi.hummingbird_ffi_mobile.MobileSurface

// `QuestionsViewModel` over fakes (ADR-0039): the list is the seam's, in
// the seam's order; Loading is real; the label is the roster's word with the
// race pane's subject appended; expansion is a session set.
class QuestionsViewModelTest {

    private fun pane(question: MobileStandingQuestion, key: String, subject: String = "") = MobileRankedPane(
        standingQuestion = question,
        subjectKey = subject,
        paneKey = key,
        answer = MobilePaneAnswer(MobilePaneAnswerState.ANSWERED, MobilePaneBand.DISTANT, null),
        facts = MobilePaneFacts.Homework(
            resolved = MobileHomeworkResolved.Facts(MobileHomeworkFacts(winner = null, others = emptyList(), daysAway = null)),
            link = null,
        ),
    )

    private val roster = listOf(
        MobileQuestionRosterEntry(MobileStandingQuestion.WASTE, "Bin collection", MobileSurface.NOW, emptyList()),
        MobileQuestionRosterEntry(MobileStandingQuestion.RACE, "Next race", MobileSurface.NOW, emptyList()),
    )

    private fun viewModel(ranked: List<MobileRankedPane>) = QuestionsViewModel(
        paneZoneQueriesFn = { emptyList() },
        rankPanesFn = { _, _ -> ranked },
        rosterFn = { roster },
    )

    @Test
    fun `before the first load there is no list, not an empty one`() {
        assertNull(viewModel(emptyList()).loaded.value)
    }

    @Test
    fun `the list is the seam's, in the seam's order, with the rank's clock`() = runBlocking {
        val ranked = listOf(
            pane(MobileStandingQuestion.RACE, "race:f1", "f1"),
            pane(MobileStandingQuestion.WASTE, "waste"),
            pane(MobileStandingQuestion.RACE, "race:motogp", "motogp"),
        )
        val vm = viewModel(ranked)
        vm.load(nowMs = 4_200L)
        assertEquals(ranked, vm.loaded.value?.panes)
        assertEquals(4_200L, vm.loaded.value?.nowMs)
    }

    @Test
    fun `labels are the roster's, and the race pane names its series`() = runBlocking {
        val vm = viewModel(emptyList())
        vm.load(1L)
        assertEquals("Bin collection", vm.label(pane(MobileStandingQuestion.WASTE, "waste")))
        assertEquals("Next race — f1", vm.label(pane(MobileStandingQuestion.RACE, "race:f1", "f1")))
    }

    @Test
    fun `toggling opens and folds one row at a time`() {
        val vm = viewModel(emptyList())
        vm.toggle("waste")
        vm.toggle("race:f1")
        assertEquals(setOf("waste", "race:f1"), vm.expanded.value)
        vm.toggle("waste")
        assertEquals(setOf("race:f1"), vm.expanded.value)
    }
}
