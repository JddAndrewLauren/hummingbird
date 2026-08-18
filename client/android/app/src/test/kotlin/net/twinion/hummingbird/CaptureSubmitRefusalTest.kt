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

    private val itemDetailViewModelSrc by lazy {
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/ItemDetailViewModel.kt")
    }

    private val itemDetailScreenSrc by lazy {
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/ItemDetailScreen.kt")
    }

    @Test
    fun `CaptureViewModel imports the real uniffi capture-decision bindings`() {
        for (binding in listOf(
            "canSubmitCapture",
            "captureMetaProblems",
            "captureFormMeta",
        )) {
            assertTrue(
                "expected an import of the generated uniffi binding $binding",
                captureViewModelSrc.contains("import uniffi.hummingbird_ffi_mobile.$binding"),
            )
        }
    }

    @Test
    fun `the production factory wires all three doors to the real bindings, not a fake`() {
        val factory = Regex("""fun create\(context: Context\)[\s\S]*?\n {4}}""")
            .find(captureViewModelSrc)
            ?.value
            ?: error("could not locate CaptureViewModel.create in the source")
        assertTrue(
            "CaptureViewModel.create must pass ::canSubmitCapture as canSubmitFn",
            factory.contains("canSubmitFn = ::canSubmitCapture"),
        )
        assertTrue(
            "CaptureViewModel.create must pass ::captureMetaProblems as metaProblemsFn",
            factory.contains("metaProblemsFn = ::captureMetaProblems"),
        )
        assertTrue(
            "CaptureViewModel.create must pass ::captureFormMeta as formMetaFn",
            factory.contains("formMetaFn = ::captureFormMeta"),
        )
    }

    /** The item edit form is the second surface to ask the same question
     * — a title it must refuse to save empty, and two free-text dates that
     * can be malformed. Both rules already exist in
     * `hummingbird_core::decisions::capture`; this pins that it reaches
     * for them rather than growing its own. */
    @Test
    fun `ItemDetailViewModel wires both draft rules to the real bindings`() {
        assertTrue(
            "expected an import of the generated canSubmitCapture binding",
            itemDetailViewModelSrc.contains("import uniffi.hummingbird_ffi_mobile.canSubmitCapture"),
        )
        assertTrue(
            "expected an import of the generated captureMetaProblems binding",
            itemDetailViewModelSrc.contains(
                "import uniffi.hummingbird_ffi_mobile.captureMetaProblems",
            ),
        )
        val factory = Regex("""fun create\(context: Context\)[\s\S]*?\n {4}}""")
            .find(itemDetailViewModelSrc)
            ?.value
            ?: error("could not locate ItemDetailViewModel.create in the source")
        assertTrue(
            "the production factory must pass ::canSubmitCapture",
            factory.contains("hasContentFn = ::canSubmitCapture"),
        )
        assertTrue(
            "the production factory must pass ::captureMetaProblems",
            factory.contains("metaProblemsFn = ::captureMetaProblems"),
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
            // The item edit form asks the same question of the same rule,
            // so the same ban applies to it.
            "ItemDetailViewModel.kt" to itemDetailViewModelSrc,
            "ItemDetailScreen.kt" to itemDetailScreenSrc,
        )) {
            assertFalse("$name must not call isBlank()", src.contains("isBlank("))
            assertFalse("$name must not call isNotBlank()", src.contains("isNotBlank("))
            assertFalse("$name must not call trim()", src.contains(".trim("))
        }
    }
}
