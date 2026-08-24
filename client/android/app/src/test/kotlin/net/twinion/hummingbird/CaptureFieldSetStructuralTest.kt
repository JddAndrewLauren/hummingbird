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
            // The item-detail pane, since the panel unification: its
            // sections render the same `ui/forms` field set over the same
            // `captureFormMeta`, so the two bans below cover it too. It was
            // exempt while it carried `SIZE_VOCABULARY`/`ENERGY_VOCABULARY`
            // of its own — that exemption is what deleting them bought.
            "ItemDetailPanel.kt" to
                "client/android/app/src/main/kotlin/net/twinion/hummingbird/ItemDetailPanel.kt",
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
        "\"@homework\"",
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
        // `DeadlineField` joined them when the dates became pickers
        // (2026-08-24): it is the deadline's whole control now, so a
        // surface that lost it would otherwise still satisfy the
        // `CaptureDateField` entry via its scheduled-date field.
        for (component in listOf(
            "LevelSlider",
            "ContextField",
            "CaptureDateField",
            "DeadlineField",
            "PriorityRow",
        )) {
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

    // ------------------------------------- the dates became pickers (2026-08-24)

    /** Operator decision 2026-08-24: neither date may raise the keyboard.
     * The twin of the Project-field pin above, and bounded the same way —
     * `readOnly = true` plus `onValueChange = {}` is what makes "picked, not
     * typed" true of the source rather than merely true of the screenshot
     * somebody took once. */
    @Test
    fun `neither date field accepts typed input`() {
        // Both of them: `CaptureDateField` is the date, and `DeadlineField`
        // carries a second read-only field of its own for the minute. Pinning
        // only the first let the test's own name over-claim.
        val bodies = mapOf(
            "CaptureDateField" to Regex("""fun CaptureDateField\([\s\S]*?\n}""")
                .find(captureFieldSrcByName.getValue("ui/forms/CaptureDateField.kt"))
                ?.value,
            "DeadlineField" to Regex("""fun DeadlineField\([\s\S]*?\n}""")
                .find(captureFieldSrcByName.getValue("ui/forms/DeadlineField.kt"))
                ?.value,
        )
        for ((name, body) in bodies) {
            checkNotNull(body) { "could not locate $name in ui/forms/$name.kt" }
            assertTrue(
                "$name must be read-only — no keyboard, ever",
                body.contains("readOnly = true"),
            )
            assertTrue(
                "$name must ignore typed input (onValueChange = {})",
                body.contains("onValueChange = {}"),
            )
            assertFalse(
                "$name must not go singleLine — it is what keeps a legacy " +
                    "free-text deadline readable instead of truncated",
                body.contains("singleLine"),
            )
            // A read-only field is reachable by a screen reader only through
            // its semantics action, and that action must *perform* the click.
            // `{ false }` reports it unhandled: TalkBack announces the label,
            // takes the double-tap and nothing opens — and `readOnly` leaves
            // no keyboard to fall back on.
            assertTrue(
                "$name's semantics onClick must perform the gesture, not just label it",
                Regex("""onClick\(label = [^)]*\) \{\s*\w+ = true\s*true\s*\}""")
                    .containsMatchIn(body.replace(Regex("//[^\n]*"), "")),
            )
        }
    }

    /** The deadline's two halves are split and rejoined by the core
     * (`hummingbird_core::decisions::urgency`, crossed on the mobile seam),
     * never by Kotlin. The date-regex ban above cannot see this one:
     * `value.substringBefore("T")` carries no regex and would sail past it,
     * and it is exactly the one-liner anybody would reach for. ADR-0025
     * forbids the second copy; this is what notices. */
    @Test
    fun `the deadline splits and joins only through the seam`() {
        val src = captureFieldSrcByName.getValue("ui/forms/DeadlineField.kt")
        for (door in listOf(
            "import uniffi.hummingbird_ffi_mobile.splitDeadline",
            "import uniffi.hummingbird_ffi_mobile.joinDeadline",
            "splitDeadline(",
            "joinDeadline(",
        )) {
            assertTrue(
                "DeadlineField must reach the core's own grammar via $door",
                src.contains(door),
            )
        }
        for (handRolled in listOf(
            "substringBefore",
            "substringAfter",
            "split(\"T\")",
            "take(10)",
            "substring(0, 10)",
        )) {
            assertFalse(
                "DeadlineField must not hand-roll the split ($handRolled) — " +
                    "splitDeadline/joinDeadline own it",
                src.contains(handRolled),
            )
        }
    }

    /** `DatePickerState.selectedDateMillis` is UTC midnight of the picked
     * civil day by Material's own contract — a calendar day wearing an
     * instant's clothes. Reading it in the device zone compiles, renders,
     * and is off by one day for every reader west of Greenwich; nothing
     * else in this module can catch that, and a hardware run at midday
     * cannot either. `WallClock` is otherwise the file that *does* read the
     * device zone, which is precisely why the picker's own conversions have
     * to be pinned against doing it. */
    @Test
    fun `the picker's millis are read in UTC, never the device zone`() {
        val src = repoFile(
            "client/android/app/src/main/kotlin/net/twinion/hummingbird/core/WallClock.kt",
        )
        // Bounded to the two conversions that claim it, not to the file:
        // `local` and `currentHourMinute` read the device zone on purpose,
        // so a file-wide search would be green whatever these two did. The
        // bound is the assertion — `client/android/README.md`'s Round 5
        // section records what an unbounded one is worth here.
        for (name in listOf("civilDate", "civilDateMillis")) {
            // Bounded by the next declaration, not by a blank line. The
            // earlier `\n\n` bound was one reformat away from swallowing
            // `currentHourMinute`, whose `ZoneId.systemDefault()` is correct
            // and deliberate — the pin would then have failed, loudly, about
            // the wrong function.
            val body = Regex("""fun $name\([\s\S]*?(?=\n\s*(/\*\*|fun |\}\s*$))""")
                .find(src)
                ?.value
                ?: error("could not locate WallClock.$name")
            assertTrue(
                "WallClock.$name must resolve a picked civil day in UTC",
                body.contains("ZoneOffset.UTC"),
            )
            assertFalse(
                "WallClock.$name must not resolve a picked civil day in the device zone — " +
                    "selectedDateMillis is UTC midnight by Material's contract",
                body.contains("ZoneId.systemDefault()"),
            )
        }
    }
}
