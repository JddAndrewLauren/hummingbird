package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// ADR-0022's two hard requirements on the capture surface's mic, gated the
// same no-emulator way `CaptureSubmitRefusalTest` and `ColorTokenDriftTest`
// gate theirs — parse the real source, not a hand-copied expectation.
//
// 1. **Dictation is on-device or it does not happen.** The ADR's whole
//    subject is that audio never leaves the device, and the *default*
//    recognition service does not establish that (the platform documents it
//    as free to use a remote server). Only `isOnDeviceRecognitionAvailable`
//    /`createOnDeviceSpeechRecognizer` do. The default-service pair is
//    therefore banned outright in this file, not merely discouraged — a
//    silent swap back is the exact regression this exists to catch, and the
//    ADR names "a network-backed recognizer as an error-path fallback" as
//    the tempting form it takes.
// 2. **No silent failure.** "The prohibition explicitly includes error
//    paths… silently unavailable is a defect against this ADR, not a nit."
//    Every way a pass can end without text has to reach the reader, so the
//    listener's `onError` may not be empty and each [DictationFailure] must
//    be raised from somewhere.
class DictationLocalityTest {

    private val captureActivitySrc by lazy {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, "client/android/app/src/main/kotlin/net/twinion/hummingbird/CaptureActivity.kt")
        check(file.isFile) { "CaptureActivity.kt not found under $root" }
        file.readText()
    }

    @Test
    fun `the recognizer is the on-device one, and the default-service pair is never reached`() {
        assertTrue(
            "must gate on SpeechRecognizer.isOnDeviceRecognitionAvailable",
            captureActivitySrc.contains("SpeechRecognizer.isOnDeviceRecognitionAvailable("),
        )
        assertTrue(
            "must construct via SpeechRecognizer.createOnDeviceSpeechRecognizer",
            captureActivitySrc.contains("SpeechRecognizer.createOnDeviceSpeechRecognizer("),
        )
        // The banned pair — checked as whole call sites so the on-device
        // spellings above do not match them.
        assertFalse(
            "SpeechRecognizer.isRecognitionAvailable selects the default (possibly remote) service",
            Regex("""SpeechRecognizer\.isRecognitionAvailable\(""").containsMatchIn(captureActivitySrc),
        )
        assertFalse(
            "SpeechRecognizer.createSpeechRecognizer selects the default (possibly remote) service",
            Regex("""SpeechRecognizer\.createSpeechRecognizer\(""").containsMatchIn(captureActivitySrc),
        )
    }

    @Test
    fun `no dictation failure path is silent`() {
        assertFalse(
            "onError must not be an empty override — an unreported error leaves a mic that looks broken",
            Regex("""override fun onError\(error: Int\)\s*\{\s*}""").containsMatchIn(captureActivitySrc),
        )
        // The enum's own declaration spells its constants bare
        // (`NO_MATCH("…")`), so any qualified `DictationFailure.X` is a use
        // site — a declared-but-never-raised case is a silent path.
        for (failure in DictationFailure.entries) {
            assertTrue(
                "DictationFailure.${failure.name} is declared but never raised",
                captureActivitySrc.contains("DictationFailure.${failure.name}"),
            )
        }
    }

    @Test
    fun `every failure the ADR names has a message`() {
        // Unavailable, refused permission, mid-session error, no match —
        // the four the ADR enumerates. Each carries the sentence the screen
        // shows, so none can be raised with nothing to display.
        for (failure in DictationFailure.entries) {
            assertTrue(
                "DictationFailure.${failure.name} has an empty message",
                failure.message.isNotEmpty(),
            )
        }
        assertTrue(
            "the screen must render the failure message",
            captureActivitySrc.contains("dictationFailure?.let"),
        )
    }
}
