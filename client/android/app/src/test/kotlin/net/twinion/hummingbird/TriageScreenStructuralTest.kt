package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The M3/#531 counterpart of `RulesScreenStructuralTest`: the gates this
// slice's own acceptance criteria most need, since nothing else catches a
// re-derived count or a smuggled-in Grill interview at review time.
class TriageScreenStructuralTest {

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }

    private fun source(name: String) =
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/$name")
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")

    private val screenSrc by lazy { source("TriageScreen.kt") }
    private val viewModelSrc by lazy { source("TriageViewModel.kt") }
    private val both by lazy {
        listOf("TriageScreen.kt" to screenSrc, "TriageViewModel.kt" to viewModelSrc)
    }

    @Test
    fun `the header counts are read from the record, never recomputed`() {
        assertTrue(
            "the header must read the board's own capturedCount",
            screenSrc.contains("board.capturedCount"),
        )
        assertTrue(
            "the header must read the board's own grillingCount",
            screenSrc.contains("board.grillingCount"),
        )
        for ((name, src) in both) {
            assertFalse(
                "$name must not recompute a count from items.size/items.count",
                Regex("""items\.(size|count)""").containsMatchIn(src),
            )
        }
    }

    @Test
    fun `the Grill button is live and navigates to the takeover, never holds its own interview state`() {
        assertTrue(
            "TriageScreen.kt must render a Grill button",
            screenSrc.contains("onGrill"),
        )
        // The button is gated on the row's own can-grill fact from the
        // seam, never a hand-rolled stage check — and it navigates rather
        // than opening an interview inline. #539 lands the takeover as a
        // separate screen/ViewModel (`GrillTakeoverScreen.kt`,
        // `GrillTakeoverViewModel.kt`); nothing here may hold a turn, a
        // session, or a grill draft state.
        for ((name, src) in both) {
            for (spelling in listOf("GrillTurn", "grillDraft", "GrillSession", "grillTurn")) {
                assertFalse(
                    "$name must not carry any Grill interview state ($spelling)",
                    src.contains(spelling),
                )
            }
        }
    }

    @Test
    fun `promoting to Ready is the only save destination offered`() {
        assertTrue(
            "TriageViewModel.kt must expose promote",
            viewModelSrc.contains("fun promote("),
        )
        // No second save path: the AC is explicit that promotion is the
        // *only* destination, so there must be no sibling "save without
        // promoting" method to offer it from.
        assertFalse(
            "TriageViewModel.kt must not offer a non-promoting save",
            viewModelSrc.contains("saveEdits"),
        )
        assertTrue(
            "triageItem must always be called with a real promoteToReady",
            viewModelSrc.contains("triageFn(itemId, true,"),
        )
    }

    @Test
    fun `mark-done goes through act, never a second triage`() {
        assertTrue(
            "TriageViewModel.kt must call the existing act path via completeFn",
            viewModelSrc.contains("\"complete\""),
        )
    }

    @Test
    fun `only one row's draft may be open at a time`() {
        assertTrue(
            "TriageViewModel.kt must track exactly one selected row",
            viewModelSrc.contains("_selectedId"),
        )
    }

    /** The route is unreachable until #532, so nothing exercises this on a
     * real device pass yet — the review finding on this PR that made this
     * gate necessary. `AlertsScreen`'s own precedent: a foreground resume
     * must re-read independent of the app-wide `syncTick`, or a capture
     * minted elsewhere while this screen was backgrounded waits for the
     * next tick to appear. */
    @Test
    fun `the screen re-reads on every foreground resume, not just on syncTick`() {
        assertTrue(
            "TriageScreen.kt must carry a LifecycleResumeEffect",
            screenSrc.contains("LifecycleResumeEffect"),
        )
        assertTrue(
            "the resume effect must reload, cancelling on pause/dispose",
            screenSrc.contains("onPauseOrDispose"),
        )
    }

    @Test
    fun `the triage route is registered and deliberately unreachable`() {
        val main = source("MainActivity.kt")
        assertTrue(
            "MainActivity must register the triage route",
            main.contains("composable(Routes.TRIAGE)"),
        )
        // #532 owns reachability. Until then nothing navigates here.
        assertFalse(
            "nothing may navigate to Routes.TRIAGE until #532 gives it a home",
            main.contains("navigate(Routes.TRIAGE)"),
        )
    }

    @Test
    fun `the editor reuses the shared form components, not a hand-rolled copy`() {
        assertTrue(screenSrc.contains("LevelSlider("))
        assertTrue(screenSrc.contains("ContextField("))
        assertTrue(screenSrc.contains("CaptureDateField("))
        // #565's review: `TriageDraft.priority` seeds from the row and
        // `toEdit` sends it, so a missing control made it unreachable
        // state rather than an omitted field.
        assertTrue(
            "the editor must offer the priority control its draft already carries",
            screenSrc.contains("PriorityRow("),
        )
        // Every vocabulary word rendered comes from the seam's own door —
        // never a Kotlin literal list of size/energy words.
        assertTrue(
            "the editor must read the seam's own form vocabulary",
            screenSrc.contains("formMeta.energies") && screenSrc.contains("formMeta.sizes"),
        )
    }

    @Test
    fun `no triage surface writes its own blank check`() {
        for ((name, src) in both) {
            assertFalse(
                "$name must not use isBlank — canSubmitCapture is the rule",
                src.contains("isBlank("),
            )
            assertFalse(
                "$name must not use isNotBlank — canSubmitCapture is the rule",
                src.contains("isNotBlank("),
            )
            assertFalse(
                "$name must not trim on the caller's behalf",
                src.contains(".trim("),
            )
        }
    }
}
