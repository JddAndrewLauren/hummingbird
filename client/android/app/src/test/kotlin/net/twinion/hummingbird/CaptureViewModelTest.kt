package net.twinion.hummingbird

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.CaptureDestination
import uniffi.hummingbird_ffi_mobile.CaptureDraft
import uniffi.hummingbird_ffi_mobile.CaptureFormMeta
import uniffi.hummingbird_ffi_mobile.MetaProblems
import uniffi.hummingbird_ffi_mobile.MobileProject
import uniffi.hummingbird_ffi_mobile.VocabOption

// CaptureViewModel.submit's control flow, exercised entirely with fakes: no
// generated JNI binding involved (a plain JVM process has no host-arch
// `.so` to load it against — see CaptureViewModel.kt's own doc, and
// CoreBindingTest's for the identical reason on the M0 binding). The
// production wiring's own correctness — that [CaptureViewModel.create]
// really reaches the real uniffi bindings and that neither this screen's
// file re-derives a rule locally (a Kotlin `isBlank()` copy or a date
// regex is an automatic reject) — is CaptureSubmitRefusalTest's job.
class CaptureViewModelTest {

    private val noProblems = MetaProblems(deadline = null, scheduledDate = null)
    private val emptyFormMeta = CaptureFormMeta(sizes = emptyList(), energies = emptyList(), suggestedContexts = emptyList())

    private fun viewModel(
        canSubmitFn: (String) -> Boolean = { it.isNotBlank() },
        metaProblemsFn: (String, String) -> MetaProblems = { _, _ -> noProblems },
        formMetaFn: () -> CaptureFormMeta = { emptyFormMeta },
        projectsFn: suspend () -> List<MobileProject> = { emptyList() },
        captureFn: suspend (CaptureDraft, Long) -> String = { _, _ -> "unused" },
    ) = CaptureViewModel(canSubmitFn, metaProblemsFn, formMetaFn, projectsFn, captureFn)

    private fun draftWithTitle(title: String) = CaptureFormState(title = title)

    @Test
    fun `canSubmit defers to the injected decision fn, not a local rule`() {
        val vm = viewModel(canSubmitFn = { it == "only this exact string" })
        vm.updateDraft(draftWithTitle("buy milk"))
        assertFalse(vm.canSubmit())
        vm.updateDraft(draftWithTitle("only this exact string"))
        assertTrue(vm.canSubmit())
    }

    @Test
    fun `submit refuses when canSubmitFn says no, and never calls captureFn`() = runBlocking {
        var captureCalled = false
        val vm = viewModel(
            canSubmitFn = { false },
            captureFn = { _, _ -> captureCalled = true; "id" },
        )
        vm.updateDraft(draftWithTitle("buy milk"))

        val didCapture = vm.submit(1_000L)

        assertFalse(didCapture)
        assertFalse("captureFn must not run for a refused draft", captureCalled)
    }

    /** The other half of [canSubmitDraft]: a title that passes but a date
     * that doesn't must refuse the same way — the same discipline
     * `ItemDetailViewModel.canSave` already applies to an edit. */
    @Test
    fun `submit refuses when a free-text date is malformed, and never calls captureFn`() = runBlocking {
        var captureCalled = false
        val vm = viewModel(
            canSubmitFn = { true },
            metaProblemsFn = { _, _ -> MetaProblems(deadline = "bad shape", scheduledDate = null) },
            captureFn = { _, _ -> captureCalled = true; "id" },
        )
        vm.updateDraft(draftWithTitle("buy milk").copy(deadline = "not-a-date"))

        val didCapture = vm.submit(1_000L)

        assertFalse(didCapture)
        assertFalse("captureFn must not run for a malformed date", captureCalled)
    }

    @Test
    fun `submit captures the current draft with the given clock when allowed`() = runBlocking {
        var seenDraft: CaptureDraft? = null
        var seenNowMs: Long? = null
        val vm = viewModel(
            canSubmitFn = { true },
            captureFn = { draft, nowMs -> seenDraft = draft; seenNowMs = nowMs; "minted-id" },
        )
        vm.updateDraft(
            CaptureFormState(
                title = "buy milk",
                destination = CaptureDestination.READY,
                size = "quick",
                energy = "low",
                context = "@errands",
            ),
        )

        val didCapture = vm.submit(42_000L)

        assertTrue(didCapture)
        assertEquals("buy milk", seenDraft?.title)
        assertEquals(CaptureDestination.READY, seenDraft?.destination)
        assertEquals("quick", seenDraft?.size)
        assertEquals("low", seenDraft?.energy)
        assertEquals("@errands", seenDraft?.context)
        assertEquals(42_000L, seenNowMs)
    }

    @Test
    fun `formMeta is read lazily from the injected door, not eagerly at construction`() {
        var calls = 0
        val vm = viewModel(formMetaFn = {
            calls++
            CaptureFormMeta(
                sizes = listOf(VocabOption(value = "quick", label = "Quick")),
                energies = emptyList(),
                suggestedContexts = listOf("@home"),
            )
        })
        assertEquals(0, calls)
        assertEquals(listOf("@home"), vm.formMeta.suggestedContexts)
        assertEquals(1, calls)
        // Read again: still just the one call — `by lazy` caches it.
        vm.formMeta
        assertEquals(1, calls)
    }

    @Test
    fun `projects is empty until loadProjects runs, then holds the injected doors answer`() = runBlocking {
        val vm = viewModel(projectsFn = { listOf(MobileProject(id = "p-1", name = "Kitchen remodel")) })
        assertEquals(emptyList<MobileProject>(), vm.projects.value)

        vm.loadProjects()

        assertEquals(listOf(MobileProject(id = "p-1", name = "Kitchen remodel")), vm.projects.value)
    }

    @Test
    fun `the mic transcript replaces only the title, verbatim`() {
        val vm = viewModel()
        vm.updateDraft(CaptureFormState(title = "stale text", context = "@home"))
        vm.onTranscript("dictated text")
        assertEquals("dictated text", vm.draft.value.title)
        assertEquals("@home", vm.draft.value.context)
    }

    // ADR-0022 requires a dictation pass that ends without text to end the
    // session *and say so*; `DictationLocalityTest` gates that every path
    // raises something, and these gate what the reader then sees.

    @Test
    fun `a dictation failure is held for the screen to show`() {
        val vm = viewModel()
        vm.onDictationFailed(DictationFailure.UNAVAILABLE)
        assertEquals(DictationFailure.UNAVAILABLE, vm.dictationFailure.value)
    }

    @Test
    fun `starting a fresh attempt clears the previous notice`() {
        // A stale "no speech recognised" hanging over a listening mic reads
        // as the new attempt having already failed.
        val vm = viewModel()
        vm.onDictationFailed(DictationFailure.NO_MATCH)
        vm.onDictationStarted()
        assertNull(vm.dictationFailure.value)
    }

    @Test
    fun `a transcript clears the notice a previous failure left`() {
        val vm = viewModel()
        vm.onDictationFailed(DictationFailure.FAILED)
        vm.onTranscript("dictated text")
        assertNull(vm.dictationFailure.value)
    }

    @Test
    fun `dictation is idle-clean before anything is attempted`() {
        assertNull(viewModel().dictationFailure.value)
    }
}
