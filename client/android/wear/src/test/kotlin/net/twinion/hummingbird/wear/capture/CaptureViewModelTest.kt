package net.twinion.hummingbird.wear.capture

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.CaptureDestination
import uniffi.hummingbird_ffi_mobile.CaptureDraft

// `CaptureViewModel.submit`'s control flow over fakes (ADR-0039): the gate
// is asked first and its "no" enqueues nothing; a "yes" captures exactly
// once and then hurries a sync exactly once, in that order. The real gate is
// the core's and never enters this process — `WearCaptureRefusalTest` pins
// that the production factory wires it.
class CaptureViewModelTest {

    private class Log {
        val events = mutableListOf<String>()
        var captured: CaptureDraft? = null
    }

    private fun viewModel(log: Log, accept: Boolean) = CaptureViewModel(
        canSubmitFn = { log.events += "gate"; accept },
        captureFn = { draft, _ -> log.events += "capture"; log.captured = draft; "item-id" },
        enqueueSyncFn = { log.events += "sync" },
    )

    @Test
    fun `a refused text captures nothing and enqueues nothing`() = runBlocking {
        val log = Log()
        assertFalse(viewModel(log, accept = false).submit("   ", 1L))
        assertEquals(listOf("gate"), log.events)
    }

    @Test
    fun `an accepted text is captured as a Triage title, then synced once`() = runBlocking {
        val log = Log()
        assertTrue(viewModel(log, accept = true).submit("buy filters", 1L))
        assertEquals(listOf("gate", "capture", "sync"), log.events)
        assertEquals("buy filters", log.captured?.title)
        assertEquals(CaptureDestination.TRIAGE, log.captured?.destination)
    }
}
