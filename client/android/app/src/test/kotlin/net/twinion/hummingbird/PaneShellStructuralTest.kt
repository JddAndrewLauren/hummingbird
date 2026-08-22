package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The pane-parity slice's own gate, over the two-form shell (`PaneShell.kt`)
// and the per-question words (`ui/panes/PaneAnswers.kt`): the shell renders
// DECIDED facts and computes none of them, the words never re-derive a
// band, the drift-gate `when`s stay exhaustive, and the glyph cap is the
// shell's to enforce. Source-text pins, because every one of these is a
// rule a fixture-driven render test would pass while broken.
class PaneShellStructuralTest {

    private fun source(name: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, "client/android/app/src/main/kotlin/net/twinion/hummingbird/$name")
        check(file.isFile) { "$name not found under $root" }
        return file.readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
    }

    private val shellSrc by lazy { source("PaneShell.kt") }
    private val answersSrc by lazy { source("ui/panes/PaneAnswers.kt") }
    private val collapseSrc by lazy { source("ui/panes/PaneCollapse.kt") }

    @Test
    fun `the words read decided facts and return no band of their own`() {
        assertFalse(
            "no function in ui/panes may RETURN a MobilePaneBand — banding is the seam's",
            Regex(""":\s*MobilePaneBand\s*[={]""").containsMatchIn(answersSrc),
        )
        assertTrue(
            "the words read the pane's decided band",
            answersSrc.contains("pane.answer.band"),
        )
        assertTrue(
            "the words read the pane's decided answer state",
            answersSrc.contains("pane.answer.answerState"),
        )
    }

    @Test
    fun `the headline dispatcher is exhaustive over every question's facts arm, with no else`() {
        for (arm in listOf(
            "MobilePaneFacts.Homework",
            "MobilePaneFacts.Waste",
            "MobilePaneFacts.Weekend",
            "MobilePaneFacts.Vacation",
            "MobilePaneFacts.Race",
            "MobilePaneFacts.Kimi",
            "MobilePaneFacts.Github",
            "MobilePaneFacts.Uptime",
            "MobilePaneFacts.Reachability",
        )) {
            assertTrue(
                "paneHeadline/paneGlyphs must answer for $arm",
                answersSrc.contains("is $arm ->"),
            )
        }
        for ((name, src) in listOf(
            "PaneAnswers.kt" to answersSrc,
            "PaneShell.kt" to shellSrc,
            "PaneCollapse.kt" to collapseSrc,
        )) {
            assertFalse(
                "$name must carry no wildcard when-arm — the exhaustive when is the drift gate",
                Regex("""(?m)^\s*else\s*->""").containsMatchIn(src),
            )
        }
    }

    @Test
    fun `the shell bounds the glyphs — the cap protects the row from the pane`() {
        assertTrue(
            "PaneShell must apply MAX_GLYPHS itself, never trust the pane",
            shellSrc.contains(".take(MAX_GLYPHS)"),
        )
    }

    @Test
    fun `the collapse default rule lives in PaneCollapse alone`() {
        assertTrue(
            "the default rule is PaneCollapse's",
            collapseSrc.contains("fun defaultCollapsed"),
        )
        for ((name, src) in listOf("PaneShell.kt" to shellSrc, "PaneAnswers.kt" to answersSrc)) {
            assertFalse(
                "$name must not re-state the collapse default (DORMANT check outside PaneCollapse)",
                src.contains("defaultCollapsed"),
            )
        }
        assertTrue(
            "an override is stamped with the band it was made in",
            collapseSrc.contains("CollapseOverride(val band: MobilePaneBand, val collapsed: Boolean)"),
        )
    }

    @Test
    fun `the ViewModels own what is open, never a remember`() {
        // The rule is one: the state that says what a reader has opened
        // lives on the ViewModel, because a `remember {}` loses it on the
        // configuration change a fold is (the recorded defect). The two
        // surfaces now hold *different* state under it — Now still has
        // per-pane, band-stamped collapse; Status has one open chip since
        // its quiet stack (#689) — so this checks each for its own.
        val nowViewModel = source("NowViewModel.kt")
        assertTrue(
            "NowViewModel must own the band-stamped override map",
            nowViewModel.contains("_paneOverrides"),
        )
        assertTrue(
            "NowViewModel must toggle through PaneCollapse.write",
            nowViewModel.contains("PaneCollapse.write("),
        )
        assertTrue(
            "NowScreen must resolve a pane's collapse through PaneCollapse.resolve",
            source("NowScreen.kt").contains("PaneCollapse.resolve("),
        )

        val statusViewModel = source("StatusViewModel.kt")
        assertTrue(
            "StatusViewModel must own the open chip",
            statusViewModel.contains("_expandedKey"),
        )
        assertTrue(
            "StatusViewModel must write the open chip through PanePrefs",
            statusViewModel.contains("writeExpandedFn"),
        )
        // The half a green build would not catch: the selection sliding back
        // into the composition, where a fold drops it.
        for (name in listOf("StatusScreen.kt", "ui/panes/StatusQuietStack.kt")) {
            assertFalse(
                "$name must hold no selection in a remember — a fold would lose it",
                Regex("""remember\s*\{""").containsMatchIn(source(name)),
            )
        }
    }

    @Test
    fun `no words re-derive an urgency window and nothing in ui-panes sorts a pane list`() {
        for ((name, src) in listOf("PaneAnswers.kt" to answersSrc, "PaneShell.kt" to shellSrc)) {
            assertFalse(
                "$name must not order a pane list — the seam's order is the order",
                src.contains("sortedBy") || src.contains("sortedWith"),
            )
        }
    }
}
