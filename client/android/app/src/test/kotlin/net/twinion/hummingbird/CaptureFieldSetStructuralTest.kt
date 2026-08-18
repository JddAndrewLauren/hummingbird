package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// #529's own acceptance criterion, verbatim: "A structural test asserts
// Kotlin holds no vocabulary literals and no date regexes, and that
// submission goes through the core's capture-problem and can-submit
// predicates." The same "parse the real source, not a hand-copied
// expectation" discipline `CaptureSubmitRefusalTest`/`NowScreenStructuralTest`
// already use for their own no-emulator gates.
class CaptureFieldSetStructuralTest {

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }

    private val captureFieldSrcByName: Map<String, String> = listOf(
        "CaptureActivity.kt" to
            "client/android/app/src/main/kotlin/net/twinion/hummingbird/CaptureActivity.kt",
        "CaptureViewModel.kt" to
            "client/android/app/src/main/kotlin/net/twinion/hummingbird/CaptureViewModel.kt",
        "ui/forms/LevelSlider.kt" to
            "client/android/app/src/main/kotlin/net/twinion/hummingbird/ui/forms/LevelSlider.kt",
        "ui/forms/ContextField.kt" to
            "client/android/app/src/main/kotlin/net/twinion/hummingbird/ui/forms/ContextField.kt",
        "ui/forms/CaptureDateField.kt" to
            "client/android/app/src/main/kotlin/net/twinion/hummingbird/ui/forms/CaptureDateField.kt",
    ).associate { (name, path) -> name to repoFile(path) }

    /** The size/energy vocabulary and the suggested contexts, as the
     * quoted string literals a hand-copy would look like. Every one of
     * these must reach `CaptureScreen` only through
     * `uniffi.hummingbird_ffi_mobile.captureFormMeta`'s `CaptureFormMeta`
     * (sizes, energies, suggestedContexts) — never typed into this
     * surface's own source, the exact drift `field-vocabulary.ts`'s header
     * documents ADR-0024 catching too late on the web side. */
    private val bannedVocabularyLiterals = listOf(
        "\"quick\"", "\"normal\"", "\"deep\"",
        "\"low\"", "\"medium\"", "\"high\"",
        "\"@home\"", "\"@computer\"", "\"@phone\"", "\"@errands\"", "\"@garden\"",
    )

    @Test
    fun `no capture surface file holds a size, energy or context vocabulary literal`() {
        for ((name, src) in captureFieldSrcByName) {
            for (literal in bannedVocabularyLiterals) {
                assertFalse(
                    "$name must not hardcode the vocabulary literal $literal — " +
                        "read it from captureFormMeta() instead",
                    src.contains(literal),
                )
            }
        }
    }

    /** A hand-rolled date shape check (`\d{4}-\d{2}-\d{2}` or similar) is
     * the Kotlin twin of a `.trim()`/`isBlank()` copy of the blank rule:
     * the core's `capture_meta_problems` is the one owner of what a valid
     * deadline/scheduled-date shape is. */
    @Test
    fun `no capture surface file hand-rolls a date-shape regex`() {
        val dateRegexSignature = Regex("""\\d\{?\d?\}?-\\d""")
        for ((name, src) in captureFieldSrcByName) {
            assertFalse(
                "$name must not hand-roll a date-shape regex — " +
                    "captureMetaProblems() is the core's own answer",
                dateRegexSignature.containsMatchIn(src) || src.contains("Regex(\"\"\"\\d"),
            )
        }
    }

    /** Submission must be gated on the real doors, not a re-derived rule —
     * `CaptureSubmitRefusalTest` already proves `create()` wires the real
     * bindings; this proves `submit()`'s own control flow actually reads
     * both `canSubmitFn` and `metaProblemsFn` (through [canSubmitDraft])
     * before ever calling `captureFn`. */
    @Test
    fun `submit gates on both canSubmitDraft and its own two injected predicates`() {
        val viewModelSrc = captureFieldSrcByName.getValue("CaptureViewModel.kt")
        val submitBody = Regex("""suspend fun submit\(nowMs: Long\)[\s\S]*?\n {4}}""")
            .find(viewModelSrc)
            ?.value
            ?: error("could not locate CaptureViewModel.submit in the source")
        assertTrue(
            "submit must refuse via canSubmitDraft before calling captureFn",
            submitBody.contains("canSubmitDraft()"),
        )
        val canSubmitDraftBody = Regex("""fun canSubmitDraft\(\)[\s\S]*?\n {4}}""")
            .find(viewModelSrc)
            ?.value
            ?: error("could not locate CaptureViewModel.canSubmitDraft in the source")
        assertTrue(
            "canSubmitDraft must read the title rule via canSubmit()/canSubmitFn",
            canSubmitDraftBody.contains("canSubmit()"),
        )
        assertTrue(
            "canSubmitDraft must read the core's date-problem predicate",
            canSubmitDraftBody.contains("metaProblems"),
        )
    }

    /** The shared form components (#529's own acceptance criterion) live
     * under `ui/forms/`, not inline in `CaptureActivity.kt` — so a second
     * screen (the Triage screen, #531) has something to import rather than
     * a second hand-copy. */
    @Test
    fun `the shared form components live under ui-forms, not inline in the capture screen`() {
        val captureActivitySrc = captureFieldSrcByName.getValue("CaptureActivity.kt")
        for (component in listOf("LevelSlider", "ContextField", "CaptureDateField")) {
            assertTrue(
                "expected CaptureActivity.kt to import the shared $component rather than " +
                    "defining its own",
                captureActivitySrc.contains("import net.twinion.hummingbird.ui.forms.$component"),
            )
        }
    }

    /** Dictation stays title-field-only (#529's own boundary, carried from
     * M1-5): the transcript callback must never touch any field but the
     * title. */
    @Test
    fun `onTranscript only ever assigns the title field`() {
        val viewModelSrc = captureFieldSrcByName.getValue("CaptureViewModel.kt")
        val body = Regex("""fun onTranscript\(transcript: String\)[\s\S]*?\n {4}}""")
            .find(viewModelSrc)
            ?.value
            ?: error("could not locate CaptureViewModel.onTranscript in the source")
        assertTrue(
            "onTranscript must set title from the transcript",
            body.contains("title = transcript"),
        )
        for (field in listOf("size =", "energy =", "context =", "description =", "priority =")) {
            assertFalse(
                "onTranscript must not also set $field",
                body.contains(field),
            )
        }
    }
}
