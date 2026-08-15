package net.twinion.hummingbird

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// CaptureViewModel.submit's control flow, exercised entirely with fakes: no
// generated JNI binding involved (a plain JVM process has no host-arch
// `.so` to load it against — see CaptureViewModel.kt's own doc, and
// CoreBindingTest's for the identical reason on the M0 binding). The
// production wiring's own correctness — that [CaptureViewModel.create]
// really reaches the real uniffi `canSubmitCapture` and that neither this
// screen's file re-derives the rule locally (a Kotlin `isBlank()` copy is
// an automatic reject) — is CaptureSubmitRefusalTest's job.
class CaptureViewModelTest {

    private fun viewModel(
        canSubmitFn: (String) -> Boolean = { it.isNotBlank() },
        captureFn: suspend (String, Long) -> String = { _, _ -> "unused" },
    ) = CaptureViewModel(canSubmitFn, captureFn)

    @Test
    fun `canSubmit defers to the injected decision fn, not a local rule`() {
        val vm = viewModel(canSubmitFn = { it == "only this exact string" })
        vm.onDraftChange("buy milk")
        assertFalse(vm.canSubmit())
        vm.onDraftChange("only this exact string")
        assertTrue(vm.canSubmit())
    }

    @Test
    fun `submit refuses when canSubmitFn says no, and never calls captureFn`() = runBlocking {
        var captureCalled = false
        val vm = viewModel(
            canSubmitFn = { false },
            captureFn = { _, _ -> captureCalled = true; "id" },
        )
        vm.onDraftChange("buy milk")

        val didCapture = vm.submit(1_000L)

        assertFalse(didCapture)
        assertFalse("captureFn must not run for a refused draft", captureCalled)
    }

    @Test
    fun `submit captures the current draft with the given clock when allowed`() = runBlocking {
        var seenTitle: String? = null
        var seenNowMs: Long? = null
        val vm = viewModel(
            canSubmitFn = { true },
            captureFn = { title, nowMs -> seenTitle = title; seenNowMs = nowMs; "minted-id" },
        )
        vm.onDraftChange("buy milk")

        val didCapture = vm.submit(42_000L)

        assertTrue(didCapture)
        assertEquals("buy milk", seenTitle)
        assertEquals(42_000L, seenNowMs)
    }

    @Test
    fun `the mic transcript replaces the draft verbatim`() {
        val vm = viewModel()
        vm.onDraftChange("stale text")
        vm.onTranscript("dictated text")
        assertEquals("dictated text", vm.draft.value)
    }
}
