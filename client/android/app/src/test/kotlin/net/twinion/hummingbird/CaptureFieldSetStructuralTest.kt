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

    private fun repoRoot(): File =
        File(
            System.getProperty("hummingbird.repoRoot")
                ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)"),
        )

    private fun repoFile(relative: String): String {
        val file = File(repoRoot(), relative)
        check(file.isFile) { "$relative not found under ${repoRoot()}" }
        return file.readText()
    }

    /** `CaptureActivity.kt`/`CaptureViewModel.kt` by name, plus every
     * `.kt` file under `ui/forms/` — enumerated from the directory rather
     * than listed by hand (review finding on #529's own PR): the whole
     * point of that directory is that #531's Triage screen adds a sixth
     * file to it, and a hardcoded five-file list would let that file
     * escape every gate below silently — the exact failure mode
     * `field-vocabulary.ts`'s header (cited below) says ADR-0024 caught
     * too late on the web side. */
    private val captureFieldSrcByName: Map<String, String> = run {
        val formsDir = File(
            repoRoot(),
            "client/android/app/src/main/kotlin/net/twinion/hummingbird/ui/forms",
        )
        check(formsDir.isDirectory) { "ui/forms directory not found under ${repoRoot()}" }
        val formsFiles = formsDir.listFiles { file -> file.extension == "kt" }
            ?.sortedBy { it.name }
            ?: error("could not list ui/forms/*.kt under $formsDir")
        check(formsFiles.isNotEmpty()) { "expected at least one .kt file under $formsDir" }

        val named = listOf(
            "CaptureActivity.kt" to
                "client/android/app/src/main/kotlin/net/twinion/hummingbird/CaptureActivity.kt",
            "CaptureViewModel.kt" to
                "client/android/app/src/main/kotlin/net/twinion/hummingbird/CaptureViewModel.kt",
        ).associate { (name, path) -> name to repoFile(path) }

        val forms = formsFiles.associate { file -> "ui/forms/${file.name}" to file.readText() }

        named + forms
    }

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
        // `PriorityRow` joined them at #565's review, when the Triage
        // editor turned out to seed a priority it had no control for.
        for (component in listOf("LevelSlider", "ContextField", "CaptureDateField", "PriorityRow")) {
            assertTrue(
                "expected CaptureActivity.kt to import the shared $component rather than " +
                    "defining its own",
                captureActivitySrc.contains("import net.twinion.hummingbird.ui.forms.$component"),
            )
        }
    }

    /** Review finding on #529's own PR: an opaque, hand-typed project id
     * was a dead-letter hazard (`items.project_id` is an FK). The details
     * disclosure's Project field must read the live list from
     * `MobileTaskHost.projects()` and offer it as a picker (`readOnly`
     * anchor field, `onValueChange = {}`) rather than accepting arbitrary
     * text. */
    @Test
    fun `the Project field is a read-only picker over the live projects list, never free text`() {
        val captureActivitySrc = captureFieldSrcByName.getValue("CaptureActivity.kt")
        val captureViewModelSrc = captureFieldSrcByName.getValue("CaptureViewModel.kt")
        assertTrue(
            "CaptureViewModel must expose the live projects list",
            captureViewModelSrc.contains("val projects: StateFlow<List<MobileProject>>"),
        )
        assertTrue(
            "CaptureViewModel.create must wire projectsFn to CoreHolder.get(...).projects()",
            captureViewModelSrc.contains(".projects()"),
        )
        val projectFieldBody = Regex("""private fun ProjectField\([\s\S]*?\n}""")
            .find(captureActivitySrc)
            ?.value
            ?: error("could not locate ProjectField in CaptureActivity.kt")
        assertTrue(
            "the Project field's anchor must be read-only",
            projectFieldBody.contains("readOnly = true"),
        )
        assertTrue(
            "the Project field's anchor must ignore typed input (onValueChange = {})",
            projectFieldBody.contains("onValueChange = {}"),
        )
        assertFalse(
            "CaptureActivity.kt must not bind an editable OutlinedTextField directly to " +
                "draft.projectId (that is the free-text hazard the picker replaces)",
            captureActivitySrc.contains("value = draft.projectId"),
        )
    }

    /** The five priority chips wrap rather than clipping at a phone width
     * — "Urgent / High / Medium / Low / No priority" does not fit one
     * 360dp row at the default font scale, let alone a scaled-up one, and
     * a fixed `Row` put the trailing priorities out of reach. No emulator
     * here, so this gates the container choice itself, exactly as
     * `NowScreenStructuralTest` does for the action buttons. */
    @Test
    fun `the priority chips wrap rather than clipping at a phone width`() {
        val priorityRowSrc = captureFieldSrcByName.getValue("ui/forms/PriorityRow.kt")
        val row = Regex("""fun PriorityRow\([\s\S]*?\n}""")
            .find(priorityRowSrc)
            ?.value
            ?: error("could not locate PriorityRow in ui/forms/PriorityRow.kt")
        assertTrue(
            "PriorityRow's chips must sit in a FlowRow, not a fixed Row",
            row.contains("FlowRow("),
        )
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
