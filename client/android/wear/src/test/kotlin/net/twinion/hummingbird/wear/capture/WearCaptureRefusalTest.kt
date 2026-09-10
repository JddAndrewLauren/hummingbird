package net.twinion.hummingbird.wear.capture

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The phone's `CaptureSubmitRefusalTest`, for the watch (ADR-0039): the
// blank rule is the core's (`canSubmitCapture`), the production factory
// wires the real binding, and no file in this package re-derives the rule
// with `isBlank`/`isNotBlank`/`.trim(` — "a Kotlin isBlank() copy is an
// automatic reject" (#503's brief) binds every client.
class WearCaptureRefusalTest {

    private fun captureDir(): File {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see client/android/build.gradle.kts)")
        val dir = File(root, "client/android/wear/src/main/kotlin/net/twinion/hummingbird/wear/capture")
        check(dir.isDirectory) { "wear capture package not found under $root" }
        return dir
    }

    private val viewModelSrc by lazy { File(captureDir(), "CaptureViewModel.kt").readText() }

    @Test
    fun `CaptureViewModel imports the real uniffi gate and the factory wires it`() {
        assertTrue(
            "expected an import of the generated uniffi binding canSubmitCapture",
            viewModelSrc.contains("import uniffi.hummingbird_ffi_mobile.canSubmitCapture"),
        )
        val factory = Regex("""fun create\(context: Context\)[\s\S]*?\n {8}}""")
            .find(viewModelSrc)?.value ?: error("could not locate CaptureViewModel.create")
        assertTrue(
            "CaptureViewModel.create must pass ::canSubmitCapture as canSubmitFn",
            factory.contains("canSubmitFn = ::canSubmitCapture"),
        )
    }

    @Test
    fun `no capture file re-derives the blank rule in Kotlin`() {
        val banned = listOf("isBlank", "isNotBlank", ".trim(")
        for (file in captureDir().listFiles { f -> f.extension == "kt" } ?: emptyArray()) {
            val src = file.readText()
                .replace(Regex("""/\*[\s\S]*?\*/"""), "")
                .replace(Regex("""(?m)^\s*//.*$"""), "")
            for (word in banned) {
                assertFalse("${file.name} must not use $word — the gate is the core's", src.contains(word))
            }
        }
    }
}
