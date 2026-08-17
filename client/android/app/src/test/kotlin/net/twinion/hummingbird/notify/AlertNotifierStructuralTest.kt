package net.twinion.hummingbird.notify

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// No emulator in this source set, so the three decisions in [AlertNotifier]
// that cannot be un-made after shipping are gated on the source itself —
// the same discipline `NowScreenStructuralTest` and `ManifestAliasTest` use.
class AlertNotifierStructuralTest {

    private val src: String by lazy {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(
            root,
            "client/android/app/src/main/kotlin/net/twinion/hummingbird/notify/AlertNotifier.kt",
        )
        check(file.isFile) { "AlertNotifier.kt not found under $root" }
        file.readText()
    }

    @Test
    fun `swiping a notification away acks nothing`() {
        // ADR-0012: dismissing a notification is not an Ack. Wiring a
        // delete intent would settle alerts the user only brushed off a
        // lock screen, and the authority would have no way to tell that
        // from a deliberate gesture.
        assertFalse(
            "AlertNotifier must wire no delete intent",
            src.contains("setDeleteIntent"),
        )
    }

    @Test
    fun `each alert posts under its own tag so a restamp replaces in place`() {
        assertTrue(
            "notify must be called with the alert id as its tag",
            Regex("""\.notify\(\s*alert\.alertId\s*,""").containsMatchIn(src),
        )
        assertTrue(
            "cancel must be tag-scoped too, or it would retire the wrong alert",
            Regex("""\.cancel\(\s*alertId\s*,""").containsMatchIn(src),
        )
    }

    @Test
    fun `the tap opens an Activity directly, never a trampoline`() {
        // Android 12+ bans starting an Activity from a service or receiver
        // woken by a notification tap; the tap intent must target the
        // Activity itself.
        assertTrue(
            "the content intent must be a getActivity PendingIntent",
            src.contains("PendingIntent.getActivity"),
        )
        assertTrue(
            "the Ack action must be a broadcast, not an Activity",
            src.contains("PendingIntent.getBroadcast"),
        )
    }

    @Test
    fun `every PendingIntent is immutable`() {
        val creations = Regex("""PendingIntent\.get\w+\(""").findAll(src).count()
        val immutable = Regex("""PendingIntent\.FLAG_IMMUTABLE""").findAll(src).count()
        assertTrue("expected at least two PendingIntents", creations >= 2)
        assertTrue(
            "every PendingIntent must carry FLAG_IMMUTABLE ($creations created, $immutable flagged)",
            immutable >= creations,
        )
    }
}
