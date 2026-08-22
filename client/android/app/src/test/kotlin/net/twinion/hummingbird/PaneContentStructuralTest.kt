package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The pane-content slice's gates — the expanded cards are renderings of
// DECIDED facts, and the one wrong thing no fixture would catch is a card
// quietly deciding for itself (a recomputed band, a second clock, a second
// dispatcher). Source gates, like every pane gate before them.
class PaneContentStructuralTest {

    private fun source(name: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, "client/android/app/src/main/kotlin/net/twinion/hummingbird/$name")
        check(file.isFile) { "$name not found under $root" }
        return file.readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
    }

    private val statusSrc by lazy { source("ui/panes/StatusPanesExpanded.kt") }
    private val nowSrc by lazy { source("ui/panes/NowPanesExpanded.kt") }

    private val quietStackSrc by lazy { source("ui/panes/StatusQuietStack.kt") }

    /** Every file that renders a pane's own content. The quiet stack joined
     * them at #689: it draws the headline and the card around the facts, so
     * the same "no second clock, no wildcard arm" rules bind it. */
    private val paneSurfaces by lazy {
        listOf(
            "StatusPanesExpanded.kt" to statusSrc,
            "NowPanesExpanded.kt" to nowSrc,
            "StatusQuietStack.kt" to quietStackSrc,
        )
    }

    @Test
    fun `the expanded cards read decided answers and return no band of their own`() {
        for ((name, src) in paneSurfaces) {
            assertFalse(
                "$name: no function may RETURN a MobilePaneBand — banding is the seam's",
                Regex(""":\s*MobilePaneBand\s*[={]""").containsMatchIn(src),
            )
        }
        assertTrue(
            "the Status cards colour by the pane's decided band",
            statusSrc.contains("pane.answer.band"),
        )
        assertTrue(
            "the Now cards branch on the pane's decided answer state",
            nowSrc.contains("pane.answer.answerState"),
        )
    }

    @Test
    fun `no expanded card reads its own clock`() {
        for ((name, src) in paneSurfaces) {
            assertFalse(
                "$name: nowMs is the shell's — a card reading the wall clock would drift from the rank",
                src.contains("System.currentTimeMillis()") ||
                    Regex("""\bDate\(\)""").containsMatchIn(src),
            )
        }
    }

    @Test
    fun `no wildcard when-arm anywhere in the expanded cards`() {
        for ((name, src) in paneSurfaces) {
            assertFalse(
                "$name: the exhaustive when is the drift gate — a new gap kind or band must fail this build",
                Regex("""(?m)^\s*else\s*->""").containsMatchIn(src),
            )
        }
    }

    @Test
    fun `the Now dispatcher answers every facts arm, renders nothing for Vacation, errors on the Status four`() {
        for (arm in listOf("Waste", "Race", "Weekend", "Vacation")) {
            assertTrue(
                "NowPaneExpanded must answer the $arm arm",
                nowSrc.contains("is MobilePaneFacts.$arm ->"),
            )
        }
        assertTrue(
            "a Status-surface question reaching the Now slot must be a loud error",
            nowSrc.contains("error(\"a Status-surface question reached the Now expanded slot"),
        )
        assertTrue(
            "Weekend renders its own card since #564/#621",
            Regex("""is MobilePaneFacts\.Weekend\s*->\s*\n?\s*WeekendPaneExpanded""")
                .containsMatchIn(nowSrc),
        )
        // Vacation still renders nothing, and for a reason that is now a
        // scope line rather than a missing lane: `MobileTrip` carries no
        // event title, so a card would name every trip by its location.
        assertTrue(
            "Vacation renders nothing beyond the shell — the seam carries no trip name",
            Regex("""is MobilePaneFacts\.Vacation\s*->\s*Unit""").containsMatchIn(nowSrc),
        )
    }

    @Test
    fun `the Now pair dispatch from NowScreen's expandedContent and nowhere else`() {
        val nowScreen = source("NowScreen.kt")
        assertTrue(
            "NowScreen must fill the shell's expandedContent slot with the one dispatcher",
            nowScreen.contains("NowPaneExpanded(pane, nowMs, onSetScheduledDate)"),
        )
        val hits = listOf("StatusScreen.kt", "NowScreen.kt", "PaneShell.kt")
            .map { it to source(it) }
            .filter { (_, src) -> src.contains("NowPaneExpanded(") }
            .map { it.first }
        assertTrue(
            "NowPaneExpanded must have exactly one caller (found: $hits)",
            hits == listOf("NowScreen.kt"),
        )
    }

    @Test
    fun `the Status dispatcher answers every facts arm and errors on the Now four`() {
        for (arm in listOf("Kimi", "Github", "Uptime", "Reachability")) {
            assertTrue(
                "StatusPaneExpanded must render the $arm arm",
                statusSrc.contains("is MobilePaneFacts.$arm ->"),
            )
        }
        assertTrue(
            "a Now-surface question reaching the Status slot must be a loud error",
            statusSrc.contains("error(\"a Now-surface question reached the Status expanded slot"),
        )
    }

    @Test
    fun `the Status four dispatch from the quiet stack and nowhere else`() {
        // The quiet stack replaced the shell's `expandedContent` slot on this
        // surface (#689): a problem card and an open chip each render the
        // pane's own facts, and both draw the headline themselves — so both
        // call the dispatcher with `headline = false`, and nothing else calls
        // it at all.
        val quietStack = source("ui/panes/StatusQuietStack.kt")
        assertTrue(
            "the quiet stack's problem card must render the pane's own facts",
            quietStack.contains("StatusPaneExpanded(pane, nowMs, headline = false)"),
        )
        assertTrue(
            "an open chip's detail must render the pane's own facts",
            quietStack.contains("StatusPaneExpanded(pane, nowMs, headline = false)"),
        )
        // Two call sites, not one: the announcing card and the open chip.
        assertTrue(
            "both hosts must draw the pane's facts",
            Regex("""StatusPaneExpanded\(pane, nowMs, headline = false\)""")
                .findAll(quietStack).count() == 2,
        )
        val hits = listOf(
            "StatusScreen.kt",
            "NowScreen.kt",
            "PaneShell.kt",
            "ui/panes/StatusQuietStack.kt",
        )
            .map { it to source(it) }
            .filter { (_, src) -> src.contains("StatusPaneExpanded(") }
            .map { it.first }
        assertTrue(
            "StatusPaneExpanded must have exactly one caller (found: $hits)",
            hits == listOf("ui/panes/StatusQuietStack.kt"),
        )
    }
}
