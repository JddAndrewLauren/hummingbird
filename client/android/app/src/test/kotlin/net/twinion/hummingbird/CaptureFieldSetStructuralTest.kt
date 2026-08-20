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
            // The FAB's capture sheet — the same surface in its light
            // form, under the same vocabulary and date-regex bans.
            "CaptureSheet.kt" to
                "client/android/app/src/main/kotlin/net/twinion/hummingbird/CaptureSheet.kt",
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
        val submitBody = Regex("""suspend fun submit\(destination: CaptureDestination, nowMs: Long\)[\s\S]*?\n {4}}""")
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
     * text. Round 4 moved the picker into `ui/forms/` — both capture
     * surfaces render the disclosure now, and the refusal has to hold on
     * both, which it cannot do from a `private fun` in one of them. */
    @Test
    fun `the Project field is a read-only picker over the live projects list, never free text`() {
        val captureViewModelSrc = captureFieldSrcByName.getValue("CaptureViewModel.kt")
        assertTrue(
            "CaptureViewModel must expose the live projects list",
            captureViewModelSrc.contains("val projects: StateFlow<List<MobileProject>>"),
        )
        assertTrue(
            "CaptureViewModel.create must wire projectsFn to CoreHolder.get(...).projects()",
            captureViewModelSrc.contains(".projects()"),
        )
        val projectFieldSrc = captureFieldSrcByName.getValue("ui/forms/ProjectField.kt")
        val projectFieldBody = Regex("""fun ProjectField\([\s\S]*?\n}""")
            .find(projectFieldSrc)
            ?.value
            ?: error("could not locate ProjectField in ui/forms/ProjectField.kt")
        assertTrue(
            "the Project field's anchor must be read-only",
            projectFieldBody.contains("readOnly = true"),
        )
        assertTrue(
            "the Project field's anchor must ignore typed input (onValueChange = {})",
            projectFieldBody.contains("onValueChange = {}"),
        )
        for (name in listOf("CaptureActivity.kt", "CaptureSheet.kt")) {
            val src = captureFieldSrcByName.getValue(name)
            assertFalse(
                "$name must not bind an editable OutlinedTextField directly to " +
                    "draft.projectId (that is the free-text hazard the picker replaces)",
                src.contains("value = draft.projectId"),
            )
            assertTrue(
                "$name renders the details disclosure, so it must import the shared picker",
                src.contains("import net.twinion.hummingbird.ui.forms.ProjectField"),
            )
        }
    }

    /** Round 4's submit pair, on both capture surfaces: the destination is
     * a property of the gesture, so each button names its own and there is
     * no `FilterChip` switch left holding a selected destination — two
     * places for one fact was the shape being replaced. Both surfaces gate
     * on `canSubmitDraft()`, not the title rule alone: the sheet's dates
     * are editable now, and `canSubmit(draft.title)` would pass a
     * malformed deadline to the authority's dead-letter journal. */
    @Test
    fun `both capture surfaces submit through two destination-carrying buttons`() {
        for (name in listOf("CaptureActivity.kt", "CaptureSheet.kt")) {
            // Comments stripped, unlike the bans above: every assertion
            // here is about what the code does, and both files explain in
            // prose the very shapes being banned ("`canSubmitDraft()`, not
            // `canSubmit(draft.title)`"), which a raw `contains` reads as
            // the defect itself.
            val src = withoutComments(captureFieldSrcByName.getValue(name))
            assertTrue(
                "$name must offer a Triage submit carrying its own destination",
                src.contains("submit(CaptureDestination.TRIAGE"),
            )
            assertTrue(
                "$name must offer an Add submit carrying its own destination",
                src.contains("submit(CaptureDestination.READY"),
            )
            assertFalse(
                "$name must not keep a FilterChip destination switch — the buttons are the choice",
                src.contains("FilterChip("),
            )
            assertTrue(
                "$name must gate submission on the whole draft, not the title rule alone",
                src.contains("canSubmitDraft()"),
            )
            assertFalse(
                "$name must not gate on canSubmit(draft.title) — a malformed date would pass",
                src.contains("canSubmit(draft.title)"),
            )
        }
        // The submit row must ride the keyboard rather than being pushed
        // under it, and the two surfaces get there by opposite means —
        // asserted per surface, because asserting `imePadding()` on both
        // was itself the bug. A bare `Scaffold` pays no IME inset, so the
        // Activity applies one; `ModalBottomSheet`'s own
        // `contentWindowInsets` defaults to `safeDrawing`, which already
        // includes the IME, so an `imePadding()` there pays it twice and
        // pushes the buttons off-screen (sighted on hardware 2026-08-20).
        assertTrue(
            "CaptureActivity must apply the IME inset itself — a bare Scaffold pays none",
            withoutComments(captureFieldSrcByName.getValue("CaptureActivity.kt"))
                .contains("imePadding()"),
        )
        assertFalse(
            "CaptureSheet must NOT apply imePadding() — ModalBottomSheet's own " +
                "contentWindowInsets already includes the IME, and paying it twice puts " +
                "the submit row under the keyboard",
            withoutComments(captureFieldSrcByName.getValue("CaptureSheet.kt"))
                .contains("imePadding()"),
        )
        // The destination is gone from the form state itself: a field no
        // control writes is state the reader can never see or correct.
        assertFalse(
            "CaptureFormState must not carry a destination field",
            withoutComments(captureFieldSrcByName.getValue("CaptureViewModel.kt"))
                .contains("val destination: CaptureDestination"),
        )
    }

    private fun withoutComments(src: String): String = src
        .replace(Regex("""/\*[\s\S]*?\*/"""), "")
        .replace(Regex("""(?m)^\s*//.*$"""), "")

    /** The priority chips lost their fifth option and their wrap on
     * 2026-08-20 (operator decision), and both halves of that — the
     * measurement at the narrowest shipped width, and the source pin
     * against a `FlowRow` coming back — live in `PriorityRowWrappingTest`,
     * which can measure a real render. This file keeps only the claim it
     * can make: the row is still one of the shared components both capture
     * surfaces render (the loop above).
     */

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
