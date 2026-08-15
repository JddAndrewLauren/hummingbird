package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The CI gate M1-5/#503's brief names explicitly: "a Kotlin isBlank() copy
// is an automatic reject." `CaptureViewModelTest` proves `submit`'s control
// flow against a fake decision function (no native call available to a
// plain JVM process — see `CaptureViewModel.kt`'s doc); this is the
// complementary, mechanical proof that the *real* wiring reaches the uniffi
// fn and that no file in the capture surface re-derives the blank rule
// itself — the same "parse the real source, not a hand-copied expectation"
// discipline `ColorTokenDriftTest` already uses for its own no-emulator gate.
class CaptureSubmitRefusalTest {

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }

    private val captureViewModelSrc by lazy {
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/CaptureViewModel.kt")
    }

    private val captureActivitySrc by lazy {
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/CaptureActivity.kt")
    }

    @Test
    fun `CaptureViewModel imports the real uniffi canSubmitCapture binding`() {
        assertTrue(
            "expected an import of the generated uniffi binding",
            captureViewModelSrc.contains("import uniffi.hummingbird_ffi_mobile.canSubmitCapture"),
        )
    }

    @Test
    fun `the production factory wires canSubmitFn to the real binding, not a fake`() {
        val factory = Regex("""fun create\(context: Context\)[\s\S]*?\n {4}}""")
            .find(captureViewModelSrc)
            ?.value
            ?: error("could not locate CaptureViewModel.create in the source")
        assertTrue(
            "CaptureViewModel.create must pass ::canSubmitCapture as canSubmitFn",
            factory.contains("canSubmitFn = ::canSubmitCapture"),
        )
    }

    @Test
    fun `neither CaptureViewModel nor CaptureActivity re-derives the blank rule locally`() {
        // A Kotlin isBlank()/trim() copy of the refusal is the exact trap
        // the brief names: it disagrees with the real rule on a BOM-only
        // draft (hummingbird_core::decisions::capture's own doc). Neither
        // banned spelling may appear anywhere in the capture surface.
        for ((name, src) in listOf(
            "CaptureViewModel.kt" to captureViewModelSrc,
            "CaptureActivity.kt" to captureActivitySrc,
        )) {
            assertFalse("$name must not call isBlank()", src.contains("isBlank("))
            assertFalse("$name must not call isNotBlank()", src.contains("isNotBlank("))
            assertFalse("$name must not call trim()", src.contains(".trim("))
        }
    }
}
