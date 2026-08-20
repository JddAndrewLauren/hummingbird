package net.twinion.hummingbird

import java.io.File
import net.twinion.hummingbird.speech.DictationFailure
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// ADR-0022's two hard requirements on the capture surfaces' mic, gated the
// same no-emulator way `CaptureSubmitRefusalTest` and `ColorTokenDriftTest`
// gate theirs — parse the real source, not a hand-copied expectation. The
// recognizer plumbing lives in `speech/Dictation.kt` since #611, extracted
// so `CaptureActivity` and the FAB's `CaptureSheet` share one copy — these
// pins target that file, plus the render pin on each host.
//
// 1. **Dictation is on-device or it does not happen.** The ADR's whole
//    subject is that audio never leaves the device, and the *default*
//    recognition service does not establish that (the platform documents it
//    as free to use a remote server). Only `isOnDeviceRecognitionAvailable`
//    /`createOnDeviceSpeechRecognizer` do. The default-service pair is
//    therefore banned outright — in the shared file, in both hosts, and in
//    `MainActivity` (which constructs the sheet's `DictationHost`), so a
//    second, local copy of the plumbing cannot sneak the fallback in — a
//    silent swap back is the exact regression this exists to catch, and the
//    ADR names "a network-backed recognizer as an error-path fallback" as
//    the tempting form it takes.
// 2. **No silent failure.** "The prohibition explicitly includes error
//    paths… silently unavailable is a defect against this ADR, not a nit."
//    Every way a pass can end without text has to reach the reader, so the
//    listener's `onError` may not be empty and each [DictationFailure] must
//    be raised from somewhere.
class DictationLocalityTest {

    private fun source(path: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, "client/android/app/src/main/kotlin/net/twinion/hummingbird/$path")
        check(file.isFile) { "$path not found under $root" }
        return file.readText()
    }

    private val dictationSrc by lazy { source("speech/Dictation.kt") }
    private val captureActivitySrc by lazy { source("CaptureActivity.kt") }
    private val captureSheetSrc by lazy { source("CaptureSheet.kt") }
    private val mainActivitySrc by lazy { source("MainActivity.kt") }

    @Test
    fun `the recognizer is the on-device one, and the default-service pair is never reached`() {
        assertTrue(
            "must gate on SpeechRecognizer.isOnDeviceRecognitionAvailable",
            dictationSrc.contains("SpeechRecognizer.isOnDeviceRecognitionAvailable("),
        )
        assertTrue(
            "must construct via SpeechRecognizer.createOnDeviceSpeechRecognizer",
            dictationSrc.contains("SpeechRecognizer.createOnDeviceSpeechRecognizer("),
        )
        // The banned pair — checked as whole call sites so the on-device
        // spellings above do not match them, and in the hosts too so no
        // second copy of the plumbing can reintroduce them.
        for ((name, src) in listOf(
            "speech/Dictation.kt" to dictationSrc,
            "CaptureActivity.kt" to captureActivitySrc,
            "CaptureSheet.kt" to captureSheetSrc,
            "MainActivity.kt" to mainActivitySrc,
        )) {
            assertFalse(
                "$name: SpeechRecognizer.isRecognitionAvailable selects the default (possibly remote) service",
                Regex("""SpeechRecognizer\.isRecognitionAvailable\(""").containsMatchIn(src),
            )
            assertFalse(
                "$name: SpeechRecognizer.createSpeechRecognizer selects the default (possibly remote) service",
                Regex("""SpeechRecognizer\.createSpeechRecognizer\(""").containsMatchIn(src),
            )
        }
    }

    @Test
    fun `no dictation failure path is silent`() {
        assertFalse(
            "onError must not be an empty override — an unreported error leaves a mic that looks broken",
            Regex("""override fun onError\(error: Int\)\s*\{\s*}""").containsMatchIn(dictationSrc),
        )
        // The enum's own declaration spells its constants bare
        // (`NO_MATCH("…")`), so any qualified `DictationFailure.X` is a use
        // site — a declared-but-never-raised case is a silent path.
        for (failure in DictationFailure.entries) {
            assertTrue(
                "DictationFailure.${failure.name} is declared but never raised",
                dictationSrc.contains("DictationFailure.${failure.name}"),
            )
        }
    }

    @Test
    fun `every failure the ADR names has a message, and both hosts render it`() {
        // Unavailable, refused permission, mid-session error, no match —
        // the four the ADR enumerates. Each carries the sentence the screen
        // shows, so none can be raised with nothing to display.
        for (failure in DictationFailure.entries) {
            assertTrue(
                "DictationFailure.${failure.name} has an empty message",
                failure.message.isNotEmpty(),
            )
        }
        for ((name, src) in listOf(
            "CaptureActivity.kt" to captureActivitySrc,
            "CaptureSheet.kt" to captureSheetSrc,
        )) {
            assertTrue(
                "$name must render the failure message",
                src.contains("dictationFailure?.let"),
            )
        }
    }
}
