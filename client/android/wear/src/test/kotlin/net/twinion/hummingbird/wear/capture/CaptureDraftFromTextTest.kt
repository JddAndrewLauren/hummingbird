package net.twinion.hummingbird.wear.capture

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.CaptureDestination

// `captureDraftFromText`'s contract (ADR-0039): the text is the title as
// given, the destination and deadline are the tapped button's, and every
// other field is empty — the watch decides nothing else at capture time.
class CaptureDraftFromTextTest {

    @Test
    fun `the text is the title, as spoken, bound where the reader tapped`() {
        val draft = captureDraftFromText("  call the plumber ", CaptureDestination.TRIAGE, "")
        assertEquals("  call the plumber ", draft.title)
        assertEquals(CaptureDestination.TRIAGE, draft.destination)
        assertEquals("", draft.deadline)
        val today = captureDraftFromText("x", CaptureDestination.READY, "2026-09-10")
        assertEquals(CaptureDestination.READY, today.destination)
        assertEquals("2026-09-10", today.deadline)
    }

    @Test
    fun `every other field is empty`() {
        val draft = captureDraftFromText("x", CaptureDestination.READY, "2026-09-10")
        for ((name, value) in listOf(
            "size" to draft.size,
            "energy" to draft.energy,
            "context" to draft.context,
            "description" to draft.description,
            "projectId" to draft.projectId,
            "priority" to draft.priority,
            "scheduledDate" to draft.scheduledDate,
            "linkUrl" to draft.linkUrl,
            "linkLabel" to draft.linkLabel,
        )) {
            assertEquals("$name must be empty on a watch capture", "", value)
        }
    }

    @Test
    fun `the landed line names the destination and the stamped day`() {
        assertEquals("TRIAGE", landedLine(CaptureDestination.TRIAGE, ""))
        assertEquals("READY", landedLine(CaptureDestination.READY, ""))
        assertEquals("READY · DUE TODAY", landedLine(CaptureDestination.READY, "2026-09-10"))
    }
}
