package net.twinion.hummingbird

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileSkillRunState

// `MicrotaskViewModel`'s control flow, with fakes only — `GrillTakeover
// ViewModelTest`'s own house shape and the same reason for the Main
// dispatcher rule.
@OptIn(ExperimentalCoroutinesApi::class)
class MicrotaskViewModelTest {

    @Before
    fun setUp() {
        Dispatchers.setMain(UnconfinedTestDispatcher())
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    private class Recorder {
        var runCalls = mutableListOf<Quad>()
        var syncCalls = 0
        var writtenSelections = mutableListOf<String>()
    }

    private data class Quad(val itemId: String, val replace: Boolean, val grain: Long?, val model: String?)

    private fun viewModel(
        recorder: Recorder,
        selection: String = "cloud",
        registryIds: List<String> = listOf("cloud"),
        modelForFn: (String) -> String? = { null },
        declinedFallbackFn: (MobileSkillRunState, String, List<String>) -> String? = { _, _, _ -> null },
        states: (Quad) -> Flow<MobileSkillRunState> = { flowOf(MobileSkillRunState.Running(emptyList())) },
    ) = MicrotaskViewModel(
        runFn = { itemId, replace, grain, model ->
            val quad = Quad(itemId, replace, grain, model)
            recorder.runCalls.add(quad)
            states(quad)
        },
        syncFn = { recorder.syncCalls += 1 },
        readSelectionFn = { selection },
        writeSelectionFn = { recorder.writtenSelections.add(it) },
        modelForFn = modelForFn,
        declinedFallbackFn = declinedFallbackFn,
        registryIds = registryIds,
    )

    @Test
    fun `a run narrates as it streams and syncs once on the terminal done`() = runTest {
        val recorder = Recorder()
        val vm = viewModel(
            recorder,
            states = {
                flowOf(
                    MobileSkillRunState.Running(listOf("reading")),
                    MobileSkillRunState.Done(listOf("reading"), "kept 2", "cloud", null),
                )
            },
        )

        vm.run("i", true, 2L)

        assertEquals(MobileSkillRunState.Done(listOf("reading"), "kept 2", "cloud", null), vm.run.value)
        assertEquals(1, recorder.syncCalls)
    }

    @Test
    fun `progress alone never triggers a sync`() = runTest {
        val recorder = Recorder()
        val vm = viewModel(recorder, states = { flowOf(MobileSkillRunState.Running(listOf("reading"))) })

        vm.run("i", false, null)

        assertEquals(0, recorder.syncCalls)
    }

    /** The in-flight lock — a duplicate tap while a run is genuinely still
     * streaming must not start a second request. `awaitCancellation()` is
     * what makes "still streaming" a real, observable suspension in this
     * JVM test rather than a `flowOf` that has already completed by the
     * time the second call happens. */
    @Test
    fun `a duplicate tap while a run is still streaming is a no-op`() = runTest {
        val recorder = Recorder()
        val vm = viewModel(
            recorder,
            states = {
                flow {
                    emit(MobileSkillRunState.Running(emptyList()))
                    awaitCancellation()
                }
            },
        )
        vm.run("i", false, null)
        assertEquals(1, recorder.runCalls.size)

        vm.run("i", false, null)

        assertEquals(1, recorder.runCalls.size)
    }

    /** #307: the seam's decline is shown verbatim, never paraphrased —
     * pinned here by identity, the same discipline
     * `GrillTakeoverViewModelTest`'s own decline test uses. */
    @Test
    fun `a decline is carried through unmodified`() = runTest {
        val recorder = Recorder()
        val reason = "That item already has 4 unticked steps. Re-run with replace to rewrite them."
        val vm = viewModel(
            recorder,
            states = { flowOf(MobileSkillRunState.Declined(emptyList(), reason, "cloud", null, true)) },
        )

        vm.run("i", false, null)

        val run = vm.run.value as MobileSkillRunState.Declined
        assertSame(reason, run.reason)
    }

    /** #274: the stored selection resolves to a `model` INSIDE `run` — no
     * caller ever supplies one, which is the review-round fix ("the AC
     * honours the sunk tier fallback is unmet"). */
    @Test
    fun `run resolves model off the current selection, never a caller literal`() = runTest {
        val recorder = Recorder()
        val vm = viewModel(
            recorder,
            selection = "home",
            modelForFn = { id -> if (id == "home") "home-model" else null },
        )

        vm.run("i", true, 3L)

        assertEquals(listOf(Quad("i", true, 3L, "home-model")), recorder.runCalls)
    }

    @Test
    fun `declinedFallbackId reads straight through the injected door`() = runTest {
        val recorder = Recorder()
        val vm = viewModel(
            recorder,
            selection = "cloud",
            registryIds = listOf("cloud", "home"),
            declinedFallbackFn = { state, selection, registryIds ->
                if (state is MobileSkillRunState.Declined && selection == "cloud" && registryIds == listOf("cloud", "home")) {
                    "home"
                } else {
                    null
                }
            },
            states = { flowOf(MobileSkillRunState.Declined(emptyList(), "down", null, null, false)) },
        )

        assertNull(vm.declinedFallbackId.value)

        vm.run("i", false, null)

        assertEquals("home", vm.declinedFallbackId.value)
    }

    /** The fallback button's one call: switches the preference AND retries
     * as the fallback tier, never two separate calls that could read a
     * stale selection in between. */
    @Test
    fun `switchAndRetry writes the fallback selection and replays the same request against it`() = runTest {
        val recorder = Recorder()
        val vm = viewModel(
            recorder,
            selection = "cloud",
            modelForFn = { id -> if (id == "home") "home-model" else null },
            declinedFallbackFn = { state, _, _ -> if (state is MobileSkillRunState.Declined) "home" else null },
            states = { quad ->
                if (quad.model == "home-model") flowOf(MobileSkillRunState.Done(emptyList(), "", "home", null))
                else flowOf(MobileSkillRunState.Declined(emptyList(), "down", null, null, false))
            },
        )
        vm.run("i", true, 2L)

        vm.switchAndRetry()

        assertEquals(listOf("home"), recorder.writtenSelections)
        assertEquals("home", vm.selection.value)
        assertEquals(
            listOf(Quad("i", true, 2L, null), Quad("i", true, 2L, "home-model")),
            recorder.runCalls,
        )
        assertEquals(MobileSkillRunState.Done(emptyList(), "", "home", null), vm.run.value)
    }

    @Test
    fun `switchAndRetry is a no-op with nothing to fall back to`() = runTest {
        val recorder = Recorder()
        val vm = viewModel(recorder, declinedFallbackFn = { _, _, _ -> null })

        vm.switchAndRetry()

        assertTrue(recorder.runCalls.isEmpty())
        assertTrue(recorder.writtenSelections.isEmpty())
    }
}
