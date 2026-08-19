package net.twinion.hummingbird.ui

import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** #557's gate — `StageBadge.test.tsx`'s pins, ported to a Robolectric
 * render (the `ChoiceRowWrappingTest` harness; its header carries the
 * `@GraphicsMode(NATIVE)` reasoning), plus the call-site half: the web test
 * file's own header explains why the worded half is pinned too — "without
 * the second half, giving every stage a glyph later would look like a
 * passing change".
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(
    sdk = [35],
    application = android.app.Application::class,
)
class StageBadgeTest {

    @get:Rule
    val composeTestRule = createComposeRule()

    @Test
    fun `the triage stage draws as a glyph that names itself, with no word`() {
        composeTestRule.setContent { StageBadge(stage = "triage", dark = false) }
        composeTestRule.onNodeWithContentDescription("Triage").assertExists()
        composeTestRule.onNodeWithText("TRIAGE").assertDoesNotExist()
    }

    @Test
    fun `every other stage keeps its dot-and-word pill`() {
        composeTestRule.setContent { StageBadge(stage = "blocked", dark = false) }
        composeTestRule.onNodeWithText("BLOCKED").assertExists()
        // The word IS the name; no second, image-role name competes with it.
        composeTestRule.onNodeWithContentDescription("Blocked").assertDoesNotExist()
    }

    @Test
    fun `compact is a dot carrying the stage name for assistive tech`() {
        composeTestRule.setContent { StageBadge(stage = "done", dark = false, compact = true) }
        composeTestRule.onNodeWithContentDescription("Done").assertExists()
        composeTestRule.onNodeWithText("DONE").assertDoesNotExist()
    }

    @Test
    fun `an unknown stage falls back to triage rather than crashing`() {
        // Web: `STAGES[stage] || STAGES.triage` — a stage this client has
        // not heard of is by definition unsorted to it.
        composeTestRule.setContent { StageBadge(stage = "someday", dark = false) }
        composeTestRule.onNodeWithContentDescription("Triage").assertExists()
    }

    // -- The structural half (source pins, `ChoiceRowWrappingTest`'s idiom).

    // Comments stripped before matching (`RecallScreenStructuralTest`'s
    // refinement) so a doc comment is free to name the thing a pin forbids.
    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
    }

    @Test
    fun `the glyph form draws no dot beside the glyph`() {
        // `StageBadge.test.tsx`'s "does not also draw the dot" — two marks
        // for one fact is the thing being prevented. The semantics tree
        // cannot see a decorative Box, so the glyph branch is pinned in
        // source: between the glyph guard and its terminal `return`, no
        // CircleShape dot is drawn.
        val src = repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/ui/StageBadge.kt")
        val glyphBranch = src.substringAfter("if (spec.glyph != null)").substringBefore("return")
        assertFalse(
            "the glyph replaces the dot as well as the word — no CircleShape dot in the glyph branch",
            glyphBranch.contains("CircleShape"),
        )
    }

    @Test
    fun `the four screens render a stage through StageBadge and no other way`() {
        // The call form with a count, never `contains("StageBadge")` — an
        // import line alone would satisfy a bare name check (the
        // `ChoiceRowWrappingTest` rule). And the ban half: no screen keeps
        // a raw `stage.uppercase()` behind the import.
        val sites = mapOf(
            "NowScreen.kt" to 1,
            "ItemDetailPanel.kt" to 1,
            "TriageScreen.kt" to 1,
            "LedgerScreen.kt" to 1,
        )
        for ((file, expected) in sites) {
            val src = repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/$file")
            val found = Regex("""StageBadge\(""").findAll(src).count()
            assertEquals(
                "$file must render its stage through StageBadge (#557) — " +
                    "expected $expected call site(s), found $found",
                expected,
                found,
            )
            assertFalse(
                "$file must not render a stage as a raw uppercased wire word (#557)",
                src.contains("stage.uppercase()"),
            )
        }
    }

    @Test
    fun `stage colours come from the drift-gated theme constants, never inline`() {
        // ADR-0026: every colour identity in the badge resolves to a
        // `Color.kt` constant `ColorTokenDriftTest` covers. A raw
        // `Color(0x...)` literal here would be a value outside the gate.
        val src = repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/ui/StageBadge.kt")
        assertFalse(
            "StageBadge.kt must not mint a colour literal of its own",
            Regex("""Color\(0x""").containsMatchIn(src),
        )
        assertTrue(
            "StageBadge.kt must import its colours from ui.theme",
            src.contains("import net.twinion.hummingbird.ui.theme."),
        )
    }
}
