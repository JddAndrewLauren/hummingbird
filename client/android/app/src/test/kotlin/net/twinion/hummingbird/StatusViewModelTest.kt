package net.twinion.hummingbird

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswer
import uniffi.hummingbird_ffi_mobile.MobileKimiGap
import uniffi.hummingbird_ffi_mobile.MobileKimiResolved
import uniffi.hummingbird_ffi_mobile.MobileHomeworkFacts
import uniffi.hummingbird_ffi_mobile.MobileHomeworkResolved
import uniffi.hummingbird_ffi_mobile.MobilePaneFacts
import uniffi.hummingbird_ffi_mobile.MobileProbeGap
import uniffi.hummingbird_ffi_mobile.MobileProbeResolved
import uniffi.hummingbird_ffi_mobile.MobileRaceGap
import uniffi.hummingbird_ffi_mobile.MobileRaceResolved
import uniffi.hummingbird_ffi_mobile.MobileRaceSetup
import uniffi.hummingbird_ffi_mobile.MobileWasteGap
import uniffi.hummingbird_ffi_mobile.MobileWasteResolved
import uniffi.hummingbird_ffi_mobile.MobileWasteSetup
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
        MobileStandingQuestion.HOMEWORK ->
            MobilePaneFacts.Homework(
                resolved = MobileHomeworkResolved.Facts(
                    facts = MobileHomeworkFacts(winner = null, others = emptyList(), daysAway = null),
                ),
                link = null,
            )
        MobileStandingQuestion.WASTE ->
            MobilePaneFacts.Waste(
                setup = MobileWasteSetup.UNSET,
                resolved = MobileWasteResolved.Gap(gap = MobileWasteGap.NotFetched),
            )
        MobileStandingQuestion.WEEKEND ->
            MobilePaneFacts.Weekend(resolved = MobileWeekendResolved.Gap(gap = MobileWeekendGap.NOT_CONNECTED))
        MobileStandingQuestion.VACATION -> MobilePaneFacts.Vacation(resolved = null)
        MobileStandingQuestion.RACE ->
            MobilePaneFacts.Race(
                setup = MobileRaceSetup.UNSET,
                resolved = MobileRaceResolved.Gap(gap = MobileRaceGap.NotFetched),
            )
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

        assertEquals(StatusState.Loaded(panes, 1_000L, null, null), vm.state.value)
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

    @Test
    fun `a failed rank is worded rather than thrown out of the screen`() = runBlocking {
        // A3c: the rank is a JNI crossing that can throw
        // `InternalException`, and this load runs inside a resume effect —
        // unhandled, it takes the Activity down. `TriageViewModel.load`'s
        // shape, applied here.
        val vm = StatusViewModel(rankPanesFn = { throw RuntimeException("mirror unreadable") })

        vm.load(1_000L)

        assertTrue(
            "a failed rank must say so",
            vm.statusLine.value?.contains("Couldn't read Status") == true,
        )
    }

    @Test
    fun `a failed rank leaves whatever the last good one rendered`() = runBlocking {
        var fail = false
        val vm = StatusViewModel(
            rankPanesFn = {
                if (fail) throw RuntimeException("mirror unreadable")
                listOf(pane(MobileStandingQuestion.KIMI, MobilePaneBand.LIVE))
            },
        )
        vm.load(1_000L)

        fail = true
        vm.load(2_000L)

        val loaded = vm.state.value as StatusState.Loaded
        assertEquals("the panes on screen must not blank on a failed reload", 1, loaded.panes.size)
        assertEquals(1_000L, loaded.rankedAtMs)
    }

    @Test
    fun `a successful rank clears a previous failure line`() = runBlocking {
        var fail = true
        val vm = StatusViewModel(
            rankPanesFn = {
                if (fail) throw RuntimeException("boom")
                emptyList()
            },
        )
        vm.load(1_000L)

        fail = false
        vm.load(2_000L)

        assertNull(vm.statusLine.value)
    }

    @Test
    fun `a load cancelled by a fold is never worded as a failure`() = runBlocking {
        val vm = StatusViewModel(rankPanesFn = { throw CancellationException("resume cancelled") })

        try {
            vm.load(1_000L)
            fail("cancellation must propagate")
        } catch (expected: CancellationException) {
        }

        assertNull(vm.statusLine.value)
    }

    // ------------------------------------------- the open chip (#689)

    @Test
    fun `opening a chip records it, and opening a second replaces the first`() = runBlocking {
        val written = mutableListOf<String?>()
        val panes = listOf(
            pane(MobileStandingQuestion.KIMI, MobilePaneBand.DORMANT),
            pane(MobileStandingQuestion.UPTIME, MobilePaneBand.DORMANT),
        )
        val vm = StatusViewModel(
            rankPanesFn = { panes },
            writeExpandedFn = { key -> written += key },
        )
        vm.load(1_000L)

        vm.toggleExpanded(panes[0])
        assertEquals(panes[0].paneKey, vm.expandedKey.value)

        // Single selection is the state's shape: there is one key, so the
        // first closes with nothing enforcing it.
        vm.toggleExpanded(panes[1])
        assertEquals(panes[1].paneKey, vm.expandedKey.value)
        assertEquals(listOf(panes[0].paneKey, panes[1].paneKey), written)
    }

    @Test
    fun `tapping the open chip again shuts it`() = runBlocking {
        val written = mutableListOf<String?>()
        val panes = listOf(pane(MobileStandingQuestion.KIMI, MobilePaneBand.DORMANT))
        val vm = StatusViewModel(
            rankPanesFn = { panes },
            writeExpandedFn = { key -> written += key },
        )
        vm.load(1_000L)

        vm.toggleExpanded(panes[0])
        vm.toggleExpanded(panes[0])

        assertNull(vm.expandedKey.value)
        assertEquals(listOf(panes[0].paneKey, null), written)
    }

    @Test
    fun `the stored open chip is read once, with the first rank`() = runBlocking {
        var reads = 0
        val vm = StatusViewModel(
            rankPanesFn = { emptyList() },
            readExpandedFn = {
                reads += 1
                "uptime:runner"
            },
        )

        vm.load(1_000L)
        vm.load(2_000L)

        assertEquals("uptime:runner", vm.expandedKey.value)
        assertEquals(1, reads)
    }

    /** A stored key whose pane no longer ranks is kept, not pruned — it
     * simply matches no chip. `PaneCollapse`'s own resurrection instinct:
     * the pane may be back on the next rank, and the reader's choice
     * should outlive one quiet cycle. */
    @Test
    fun `a stored key that no longer ranks is kept rather than pruned`() = runBlocking {
        val written = mutableListOf<String?>()
        val vm = StatusViewModel(
            rankPanesFn = { listOf(pane(MobileStandingQuestion.KIMI, MobilePaneBand.DORMANT)) },
            readExpandedFn = { "uptime:a-service-that-stopped-ranking" },
            writeExpandedFn = { key -> written += key },
        )

        vm.load(1_000L)

        assertEquals("uptime:a-service-that-stopped-ranking", vm.expandedKey.value)
        assertEquals(emptyList<String?>(), written)
    }

    @Test
    fun `the queue depth and api version ride the same crossing as the rank`() = runBlocking {
        val vm = StatusViewModel(
            rankPanesFn = { emptyList() },
            queueDepthFn = { 3u },
            apiVersionFn = { 4u },
        )

        vm.load(1_000L)

        val loaded = vm.state.value as StatusState.Loaded
        assertEquals(3u, loaded.queueDepth)
        assertEquals(4u, loaded.apiVersion)
    }
}
