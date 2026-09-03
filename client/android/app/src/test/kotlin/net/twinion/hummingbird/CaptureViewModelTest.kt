package net.twinion.hummingbird

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import net.twinion.hummingbird.speech.DictationFailure
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
import uniffi.hummingbird_ffi_mobile.ShareDraftRecord
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

        val didCapture = vm.submit(CaptureDestination.TRIAGE, 1_000L)

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

        val didCapture = vm.submit(CaptureDestination.TRIAGE, 1_000L)

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
                size = "quick",
                energy = "low",
                context = "@errands",
            ),
        )

        val didCapture = vm.submit(CaptureDestination.READY, 42_000L)

        assertTrue(didCapture)
        assertEquals("buy milk", seenDraft?.title)
        assertEquals(CaptureDestination.READY, seenDraft?.destination)
        assertEquals("quick", seenDraft?.size)
        assertEquals("low", seenDraft?.energy)
        assertEquals("@errands", seenDraft?.context)
        assertEquals(42_000L, seenNowMs)
    }

    /** The destination is the gesture's, not the form's (round 4's two
     * submit buttons): the same draft submitted through either button must
     * reach the seam carrying that button's destination and nothing else. */
    @Test
    fun `each submit carries its own destination, and the draft holds none`() = runBlocking {
        val seen = mutableListOf<CaptureDestination>()
        val vm = viewModel(
            canSubmitFn = { true },
            captureFn = { draft, _ -> seen.add(draft.destination); "minted-id" },
        )
        vm.updateDraft(draftWithTitle("buy milk"))

        assertTrue(vm.submit(CaptureDestination.TRIAGE, 1_000L))
        assertTrue(vm.submit(CaptureDestination.READY, 2_000L))

        assertEquals(listOf(CaptureDestination.TRIAGE, CaptureDestination.READY), seen)
    }

    /** Two submit buttons plus the title field's IME action are three doors
     * onto one `captureFn`, so a second tap inside the first's suspension
     * would mint the same words twice — and the duplicate is
     * indistinguishable from a deliberate one (`submitting`'s own doc).
     * `captureFn` is parked on a `CompletableDeferred` here, which is what
     * a real local-first enqueue plus its sync leg looks like from the
     * caller's side. */
    @Test
    fun `a second submit while the first is in flight refuses, and captures once`() = runBlocking {
        val parked = CompletableDeferred<Unit>()
        var captureCalls = 0
        val vm = viewModel(
            canSubmitFn = { true },
            captureFn = { _, _ -> captureCalls++; parked.await(); "minted-id" },
        )
        vm.updateDraft(draftWithTitle("buy milk"))

        var firstResult: Boolean? = null
        val first = launch { firstResult = vm.submit(CaptureDestination.READY, 1_000L) }
        // Let the first reach `captureFn` and park there.
        while (captureCalls == 0) {
            yield()
        }
        assertTrue("the flag must be up while captureFn is in flight", vm.submitting.value)

        assertFalse(
            "a second submit while one is in flight must refuse",
            vm.submit(CaptureDestination.TRIAGE, 2_000L),
        )
        assertEquals("captureFn must have run exactly once", 1, captureCalls)

        parked.complete(Unit)
        first.join()
        assertEquals(true, firstResult)
        assertFalse("the flag must come back down once the capture lands", vm.submitting.value)

        // And a fresh submit is allowed again.
        assertTrue(vm.submit(CaptureDestination.TRIAGE, 3_000L))
        assertEquals(2, captureCalls)
    }

    /** The flag comes down even when the enqueue throws — a `finally`, not
     * a happy-path reset: a `captureFn` that failed once must not leave
     * both buttons dead for the rest of the screen's life. */
    @Test
    fun `a failed capture releases the in-flight flag`() = runBlocking {
        val vm = viewModel(
            canSubmitFn = { true },
            captureFn = { _, _ -> error("enqueue failed") },
        )
        vm.updateDraft(draftWithTitle("buy milk"))

        runCatching { vm.submit(CaptureDestination.READY, 1_000L) }

        assertFalse(vm.submitting.value)
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

    /** The sheet's post-submit reset: `MainActivity`'s store outlives the
     * sheet, so without this the next open would replay the submitted
     * capture's words as a fresh draft (clearDraft's own doc). */
    @Test
    fun `clearDraft resets the draft to its resting state`() {
        val vm = viewModel()
        vm.updateDraft(
            CaptureFormState(
                title = "buy milk",
                context = "@errands",
                deadline = "2026-08-20",
            ),
        )

        vm.clearDraft()

        assertEquals(CaptureFormState(), vm.draft.value)
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

    // #782: the share target's seed, and the Link's one refusal.

    /** The seed lands the core's answer field for field and opens the link
     * disclosure when a URL arrived — the reader sees what is about to be
     * saved. Nothing else in the draft is touched. */
    @Test
    fun `a share seeds title, description and URL, and opens the link disclosure`() {
        val vm = viewModel()
        vm.updateDraft(CaptureFormState(context = "@computer"))

        vm.seedFromShare(
            ShareDraftRecord(
                title = "Knee rehab video",
                description = "Watch this later",
                linkUrl = "https://www.youtube.com/watch?v=abc",
            ),
        )

        val draft = vm.draft.value
        assertEquals("Knee rehab video", draft.title)
        assertEquals("Watch this later", draft.description)
        assertEquals("https://www.youtube.com/watch?v=abc", draft.linkUrl)
        assertEquals("", draft.linkLabel)
        assertTrue("a URL opens the disclosure", draft.linkOpen)
        assertEquals("the rest of the draft is untouched", "@computer", draft.context)
    }

    @Test
    fun `a share without a URL leaves the link disclosure shut`() {
        val vm = viewModel()
        vm.seedFromShare(ShareDraftRecord(title = "A thought", description = "", linkUrl = ""))
        assertFalse(vm.draft.value.linkOpen)
        assertEquals("A thought", vm.draft.value.title)
    }

    /** `LaunchedEffect(Unit)` re-fires on an Activity recreation, so the
     * seed must be idempotent or a rotation overwrites the reader's edits
     * with the share's original words. */
    @Test
    fun `a second seed never overwrites what the reader edited after the first`() {
        val vm = viewModel()
        val share = ShareDraftRecord(title = "Shared title", description = "", linkUrl = "https://example.test/")
        vm.seedFromShare(share)
        vm.updateDraft(vm.draft.value.copy(title = "Edited title", linkLabel = "Example"))

        vm.seedFromShare(share)

        assertEquals("Edited title", vm.draft.value.title)
        assertEquals("Example", vm.draft.value.linkLabel)
    }

    /** A link name beside no URL is the authority's 400; it is refused
     * here first, and captureFn never runs. */
    @Test
    fun `a link name without a URL refuses the submit, and never calls captureFn`() = runBlocking {
        var captureCalled = false
        val vm = viewModel(
            canSubmitFn = { true },
            captureFn = { _, _ -> captureCalled = true; "id" },
        )
        vm.updateDraft(draftWithTitle("buy milk").copy(linkLabel = "Shop"))

        assertFalse(vm.canSubmitDraft())
        assertFalse(vm.submit(CaptureDestination.TRIAGE, 1_000L))
        assertFalse("captureFn must not run for a stranded link name", captureCalled)

        vm.updateDraft(vm.draft.value.copy(linkUrl = "https://shop.example.test/"))
        assertTrue("the same name beside a URL is fine", vm.canSubmitDraft())
    }

    /** Both halves reach the seam's draft. */
    @Test
    fun `the link reaches the seam draft`() = runBlocking {
        var seenDraft: CaptureDraft? = null
        val vm = viewModel(canSubmitFn = { true }, captureFn = { draft, _ -> seenDraft = draft; "id" })
        vm.updateDraft(
            draftWithTitle("buy milk").copy(linkUrl = "https://shop.example.test/", linkLabel = "Shop"),
        )

        assertTrue(vm.submit(CaptureDestination.READY, 1_000L))

        assertEquals("https://shop.example.test/", seenDraft?.linkUrl)
        assertEquals("Shop", seenDraft?.linkLabel)
    }
}
