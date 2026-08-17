package net.twinion.hummingbird.notify

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// The FCM payload's device-side mapping (M2/#141). The wire shape is fixed
// by `authority/src/fcm.rs::message_json` — all-string `data`, with `body`
// **omitted** rather than emptied when there is none — and every assertion
// here is against that shape, not against a convenient one.
class AlertNotificationTest {

    private fun payload(
        alertId: String? = "alert-1",
        title: String? = "Sweeper run failed",
        body: String? = "Google Tasks adapter returned 503 twice.",
        channelId: String? = "urgent",
        extra: Map<String, String> = emptyMap(),
    ): Map<String, String> = buildMap {
        alertId?.let { put("alert_id", it) }
        title?.let { put("title", it) }
        body?.let { put("body", it) }
        channelId?.let { put("channel_id", it) }
        put("severity", "error")
        put("tier", "urgent")
        putAll(extra)
    }

    @Test
    fun `a full urgent payload maps to every field`() {
        val mapped = AlertNotification.from(payload())!!

        assertEquals("alert-1", mapped.alertId)
        assertEquals("urgent", mapped.channelId)
        assertEquals("Sweeper run failed", mapped.title)
        assertEquals("Google Tasks adapter returned 503 twice.", mapped.body)
    }

    @Test
    fun `an omitted body stays null -- it is not the same input as a blank one`() {
        // `data` has no null, so the server omits the key entirely; a
        // bodyless notification renders as one line, a blank-bodied one as
        // two. Coercing absent to "" here would quietly pick the wrong one.
        assertNull(AlertNotification.from(payload(body = null))!!.body)
        assertEquals("", AlertNotification.from(payload(body = ""))!!.body)
    }

    @Test
    fun `a payload with no alert id maps to nothing showable`() {
        // Every action on the notification is keyed by alert id; without
        // one there is nothing to ack and nothing to deep-link to. The
        // caller still syncs — that is HbMessagingService's own contract.
        assertNull(AlertNotification.from(payload(alertId = null)))
        assertNull(AlertNotification.from(payload(alertId = "  ")))
    }

    @Test
    fun `a payload with no title maps to nothing showable`() {
        assertNull(AlertNotification.from(payload(title = null)))
        assertNull(AlertNotification.from(payload(title = "")))
    }

    @Test
    fun `an unknown or missing channel falls back to normal, never up to urgent`() {
        // Guessing upward would let a malformed or future-tier payload
        // bypass DND. Under-loud is the recoverable failure.
        assertEquals("normal", AlertNotification.from(payload(channelId = null))!!.channelId)
        assertEquals("normal", AlertNotification.from(payload(channelId = "critical"))!!.channelId)
        assertEquals("normal", AlertNotification.from(payload(channelId = ""))!!.channelId)
    }

    @Test
    fun `both server-emitted channel ids survive the mapping`() {
        for (spec in NotificationChannels.SPECS) {
            assertEquals(
                "channel ${spec.id} must map to itself",
                spec.id,
                AlertNotification.from(payload(channelId = spec.id))!!.channelId,
            )
        }
    }

    @Test
    fun `the same payload maps identically however many times it arrives`() {
        // FCM re-sends, and the restamp of an unchanged alert must produce
        // a byte-identical notification so posting it *replaces* the old
        // one silently instead of re-ringing a dismissal away.
        val data = payload()
        assertEquals(AlertNotification.from(data), AlertNotification.from(data))
    }

    @Test
    fun `the mapper reads no clock -- a relative phrase would defeat every restamp`() {
        // The structural half of the test above: an equality check passes
        // within one millisecond of itself, so it cannot catch "2h ago"
        // baked into a title. This can. Any time source in this file is
        // the defect.
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val src = File(
            root,
            "client/android/app/src/main/kotlin/net/twinion/hummingbird/notify/AlertNotification.kt",
        )
        assertTrue("AlertNotification.kt not found under $root", src.isFile)
        val text = src.readText()
        for (token in listOf(
            "System.currentTimeMillis",
            "java.time",
            "LocalDateTime",
            "Instant",
            "Calendar",
            "SystemClock",
            "Date(",
        )) {
            assertFalse(
                "AlertNotification.kt must read no clock, but mentions $token",
                text.contains(token),
            )
        }
    }
}
