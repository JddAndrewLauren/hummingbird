package net.twinion.hummingbird.wear.capture

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.CaptureDestination

// `captureDraftFromText`'s contract (ADR-0039): the text is the title as
// given, the destination is Triage, and every other field is empty — the
// watch decides nothing at capture time.
class CaptureDraftFromTextTest {

    @Test
    fun `the text is the title, as spoken`() {
        val draft = captureDraftFromText("  call the plumber ")
        assertEquals("  call the plumber ", draft.title)
        assertEquals(CaptureDestination.TRIAGE, draft.destination)
    }

    @Test
    fun `every other field is empty`() {
        val draft = captureDraftFromText("x")
        for ((name, value) in listOf(
            "size" to draft.size,
            "energy" to draft.energy,
            "context" to draft.context,
            "description" to draft.description,
            "projectId" to draft.projectId,
            "priority" to draft.priority,
            "deadline" to draft.deadline,
            "scheduledDate" to draft.scheduledDate,
            "linkUrl" to draft.linkUrl,
            "linkLabel" to draft.linkLabel,
        )) {
            assertEquals("$name must be empty on a watch capture", "", value)
        }
    }
}
