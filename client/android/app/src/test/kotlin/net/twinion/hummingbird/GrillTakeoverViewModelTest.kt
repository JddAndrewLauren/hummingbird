package net.twinion.hummingbird

import androidx.lifecycle.ViewModelStore
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.ItemStepRecord
import uniffi.hummingbird_ffi_mobile.MobileGrillCompletion
import uniffi.hummingbird_ffi_mobile.MobileGrillProposal
import uniffi.hummingbird_ffi_mobile.MobileGrillQuestion
import uniffi.hummingbird_ffi_mobile.MobileGrillTurn
import uniffi.hummingbird_ffi_mobile.MobileGrillTurnState
import uniffi.hummingbird_ffi_mobile.MobileGrillVerdict

// `GrillTakeoverViewModel`'s control flow, with fakes only — no uniffi
// binding is called here (`ItemDetailViewModelTest`'s own house shape).
// `viewModelScope` needs a Main dispatcher in a plain JVM process, hence
// the `UnconfinedTestDispatcher`: eager enough that `open()`/`answer()`/
// `retry()` land their effects before the assertion line runs, with no
// `advanceUntilIdle()` needed for a fake `grillTurn` that never suspends
// for real.
@OptIn(ExperimentalCoroutinesApi::class)
class GrillTakeoverViewModelTest {

    @Before
    fun setUp() {
        Dispatchers.setMain(UnconfinedTestDispatcher())
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    private fun question(prompt: String = "Which airport?") = MobileGrillQuestion(
        prompt = prompt,
        recommendedAnswer = "SEA",
        choices = listOf("SEA", "PDX"),
    )

    private fun proposal() = MobileGrillProposal(
        summary = "Settled on SEA",
        verdict = MobileGrillVerdict.RESOLVED,
        patchJson = "{}",
    )

    private class Recorder {
        var savedDrafts = mutableListOf<Pair<String, List<MobileGrillTurn>>>()
        var discardedItemIds = mutableListOf<String>()
        var itemDetailCalls = 0
        var grillTurnCalls = mutableListOf<List<MobileGrillTurn>>()
    }

    private fun viewModel(
        recorder: Recorder,
        draft: List<MobileGrillTurn>? = null,
        completeGrillFn: suspend (String, List<ItemStepRecord>, MobileGrillCompletion, Long) -> String =
            { _, _, _, _ -> "grill-1" },
        grillTurnStates: (List<MobileGrillTurn>) -> Flow<MobileGrillTurnState> = { flowOf(MobileGrillTurnState.Asking(emptyList())) },
        saveDraftFn: suspend (String, List<MobileGrillTurn>, Long) -> Unit =
            { id, turns, _ -> recorder.savedDrafts.add(id to turns) },
    ) = GrillTakeoverViewModel(
        itemDetailFn = { id, _ ->
            recorder.itemDetailCalls += 1
            itemDetail(id)
        },
        grillDraftFn = { _ -> draft },
        saveDraftFn = saveDraftFn,
        discardDraftFn = { id, _ -> recorder.discardedItemIds.add(id) },
        completeGrillFn = completeGrillFn,
        grillTurn = { _, turns ->
            recorder.grillTurnCalls.add(turns)
            grillTurnStates(turns)
        },
    )

    @Test
    fun `open loads the item and lands on the first question`() = runTest {
        val recorder = Recorder()
        val vm = viewModel(
            recorder,
            grillTurnStates = { flowOf(MobileGrillTurnState.Question(emptyList(), question(), "cloud", null)) },
        )

        vm.open("i", 1_000)

        val state = vm.state.value as GrillTakeoverState.Ready
        assertEquals("i", state.item.id)
        assertTrue(state.turn is MobileGrillTurnState.Question)
        assertEquals(1, recorder.itemDetailCalls)
    }

    @Test
    fun `resuming a saved draft threads its turns into the very first request`() = runTest {
        val recorder = Recorder()
        val savedTurn = MobileGrillTurn(question("Which dates?"), "September")
        val vm = viewModel(
            recorder,
            draft = listOf(savedTurn),
            grillTurnStates = { flowOf(MobileGrillTurnState.Question(emptyList(), question(), null, null)) },
        )

        vm.open("i", 1_000)

        assertEquals(listOf(listOf(savedTurn)), recorder.grillTurnCalls)
        val state = vm.state.value as GrillTakeoverState.Ready
        assertEquals(listOf(savedTurn), state.turns)
    }

    /** #539's own AC: a draft survives backgrounding because every
     * completed round is saved as it lands — never deferred to Back or a
     * lifecycle callback that a killed process might never run. */
    @Test
    fun `a completed round saves the draft with the turns that produced it`() = runTest {
        val recorder = Recorder()
        val q1 = question("Which airport?")
        val nextQuestion = question("Which dates?")
        val vm = viewModel(
            recorder,
            grillTurnStates = { turns ->
                if (turns.isEmpty()) flowOf(MobileGrillTurnState.Question(emptyList(), q1, null, null))
                else flowOf(MobileGrillTurnState.Question(emptyList(), nextQuestion, null, null))
            },
        )
        vm.open("i", 1_000)

        vm.answer("i", "SEA")

        val expectedTurns = listOf(MobileGrillTurn(q1, "SEA"))
        assertEquals(listOf("i" to expectedTurns), recorder.savedDrafts)
    }

    /** #565's review: the save must not wait on the answer coming back. A
     * decline (or a hang, or a process death while `Asking`) leaves the
     * round the human already typed, and saving only once a fresh
     * `Question`/`Proposal` landed lost it — reopening then resumed an
     * older transcript. */
    @Test
    fun `a round whose request declines still persists the answer`() = runTest {
        val recorder = Recorder()
        val q1 = question("Which airport?")
        val vm = viewModel(
            recorder,
            grillTurnStates = { turns ->
                if (turns.isEmpty()) flowOf(MobileGrillTurnState.Question(emptyList(), q1, null, null))
                else flowOf(MobileGrillTurnState.Declined(emptyList(), "no runner", null, null, true))
            },
        )
        vm.open("i", 1_000)

        vm.answer("i", "SEA")

        assertEquals(listOf("i" to listOf(MobileGrillTurn(q1, "SEA"))), recorder.savedDrafts)
    }

    /** #566's wrap-up: answering and immediately pressing Back must not
     * lose the round. The pop clears this entry's `ViewModelStore`, which
     * cancels `viewModelScope` — the same lifetime trap `discard` answers
     * by making the caller wait, except there is no navigation to sequence
     * against here, only a write that has to finish. Back saves nothing
     * itself, so this is the round's only chance to reach disk.
     *
     * `ViewModelStore.clear()` is the pop, exactly: it is what
     * `NavBackStackEntry` calls, and it is what invokes `ViewModel.clear()`
     * and cancels the scope. A plain `viewModelScope.launch` fails this. */
    @Test
    fun `a round answered just before the back-stack entry is popped still reaches disk`() = runTest {
        val recorder = Recorder()
        val q1 = question("Which airport?")
        val saveReached = CompletableDeferred<Unit>()
        val vm = viewModel(
            recorder,
            grillTurnStates = { turns ->
                if (turns.isEmpty()) flowOf(MobileGrillTurnState.Question(emptyList(), q1, null, null))
                else flowOf(MobileGrillTurnState.Asking(emptyList()))
            },
            // Suspends until the test lets it through, standing in for the
            // real `Core::saveGrillDraft` still in flight when the pop lands.
            saveDraftFn = { id, turns, _ ->
                saveReached.await()
                recorder.savedDrafts.add(id to turns)
            },
        )
        val store = ViewModelStore()
        store.put("grill", vm)
        vm.open("i", 1_000)

        vm.answer("i", "SEA")
        store.clear()
        saveReached.complete(Unit)
        advanceUntilIdle()

        assertEquals(
            "the answered round must survive the pop that cancels viewModelScope",
            listOf("i" to listOf(MobileGrillTurn(q1, "SEA"))),
            recorder.savedDrafts,
        )
    }

    /** No round ever answered yet (a fresh, just-opened session) must not
     * mint a resumable draft — the identical rule the web's continuous-save
     * effect states for itself. */
    @Test
    fun `an empty-turns question never saves a draft`() = runTest {
        val recorder = Recorder()
        val vm = viewModel(
            recorder,
            grillTurnStates = { flowOf(MobileGrillTurnState.Question(emptyList(), question(), null, null)) },
        )

        vm.open("i", 1_000)

        assertTrue(recorder.savedDrafts.isEmpty())
    }

    /** "Try again" re-asks with the transcript already threaded — never a
     * fresh, empty interview. */
    @Test
    fun `retry re-asks with the exact turns the decline carried`() = runTest {
        val recorder = Recorder()
        val q1 = question()
        val turns = listOf(MobileGrillTurn(q1, "SEA"))
        val vm = viewModel(
            recorder,
            draft = turns,
            grillTurnStates = { flowOf(MobileGrillTurnState.Declined(emptyList(), "nope", null, null, true)) },
        )
        vm.open("i", 1_000)
        recorder.grillTurnCalls.clear()

        vm.retry("i")

        assertEquals(listOf(turns), recorder.grillTurnCalls)
    }

    /** #307: the seam's decline prose crosses verbatim, unprefixed. */
    @Test
    fun `a decline renders the seams reason verbatim`() = runTest {
        val recorder = Recorder()
        val reason = "No device token on this device. Enter one in Settings, then try again."
        val vm = viewModel(
            recorder,
            grillTurnStates = { flowOf(MobileGrillTurnState.Declined(emptyList(), reason, null, null, false)) },
        )

        vm.open("i", 1_000)

        val state = vm.state.value as GrillTakeoverState.Ready
        val turn = state.turn as MobileGrillTurnState.Declined
        assertSame(reason, turn.reason)
    }

    @Test
    fun `keepGrilling clears any prior completion error and re-asks the same turns`() = runTest {
        val recorder = Recorder()
        val q1 = question()
        val turns = listOf(MobileGrillTurn(q1, "SEA"))
        val vm = viewModel(
            recorder,
            draft = turns,
            grillTurnStates = { flowOf(MobileGrillTurnState.Proposal(emptyList(), proposal(), null, null)) },
        )
        vm.open("i", 1_000)
        recorder.grillTurnCalls.clear()

        vm.keepGrilling("i")

        assertEquals(listOf(turns), recorder.grillTurnCalls)
        val state = vm.state.value as GrillTakeoverState.Ready
        assertNull(state.completionError)
    }

    @Test
    fun `confirm on ok discards the draft and answers true`() = runTest {
        val recorder = Recorder()
        val vm = viewModel(
            recorder,
            grillTurnStates = { flowOf(MobileGrillTurnState.Proposal(emptyList(), proposal(), null, null)) },
            completeGrillFn = { _, _, _, _ -> "grill-1" },
        )
        vm.open("i", 1_000)

        val ok = vm.confirm(
            "i",
            MobileGrillCompletion("Q: p\nA: SEA", "s", MobileGrillVerdict.RESOLVED, "{}", "{}", false),
            2_000,
        )

        assertTrue(ok)
        assertEquals(listOf("i"), recorder.discardedItemIds)
        assertFalse((vm.state.value as GrillTakeoverState.Ready).confirming)
    }

    @Test
    fun `confirm on failure sets the completion error and never discards`() = runTest {
        val recorder = Recorder()
        val vm = viewModel(
            recorder,
            grillTurnStates = { flowOf(MobileGrillTurnState.Proposal(emptyList(), proposal(), null, null)) },
            completeGrillFn = { _, _, _, _ -> error("the item's steps changed since this review started") },
        )
        vm.open("i", 1_000)

        val ok = vm.confirm(
            "i",
            MobileGrillCompletion("Q: p\nA: SEA", "s", MobileGrillVerdict.RESOLVED, "{}", "{}", false),
            2_000,
        )

        assertFalse(ok)
        assertTrue(recorder.discardedItemIds.isEmpty())
        val state = vm.state.value as GrillTakeoverState.Ready
        assertFalse(state.confirming)
        assertEquals("the item's steps changed since this review started", state.completionError)
    }

    /** The Activity-recreation fix (#539 review): a second `open()` call
     * for the SAME item — exactly what a fresh `LaunchedEffect(itemId)`
     * fires after a fold/rotation rebuilds the composition — must not reset
     * an in-progress interview to Loading or re-issue a billed request. */
    @Test
    fun `open is a no-op once already open over the same item`() = runTest {
        val recorder = Recorder()
        val vm = viewModel(
            recorder,
            grillTurnStates = { flowOf(MobileGrillTurnState.Question(emptyList(), question(), null, null)) },
        )
        vm.open("i", 1_000)
        val afterFirstOpen = vm.state.value

        vm.open("i", 2_000)

        assertEquals(1, recorder.itemDetailCalls)
        assertEquals(1, recorder.grillTurnCalls.size)
        assertSame(afterFirstOpen, vm.state.value)
    }

    /** The same guard must not block a genuine navigation to a different
     * item — only the identical-item re-fire is suppressed. */
    @Test
    fun `open over a different item re-fetches and re-asks fresh`() = runTest {
        val recorder = Recorder()
        val vm = viewModel(
            recorder,
            grillTurnStates = { flowOf(MobileGrillTurnState.Question(emptyList(), question(), null, null)) },
        )
        vm.open("i", 1_000)

        vm.open("other", 2_000)

        assertEquals(2, recorder.itemDetailCalls)
        val state = vm.state.value as GrillTakeoverState.Ready
        assertEquals("other", state.item.id)
    }

    @Test
    fun `discard carries out the drop once asked`() = runTest {
        val recorder = Recorder()
        val vm = viewModel(recorder)

        vm.discard("i", 1_000)

        assertEquals(listOf("i"), recorder.discardedItemIds)
    }
}
