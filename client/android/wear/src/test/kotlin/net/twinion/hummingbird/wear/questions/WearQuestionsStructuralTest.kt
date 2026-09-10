package net.twinion.hummingbird.wear.questions

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The watch's pane gates (ADR-0039), the phone's `PaneShellStructuralTest`
// and `PaneContentStructuralTest` rules re-applied: the list is the seam's
// order (nothing sorts), the cards read decided facts (nothing returns a
// band, no card reads its own clock), every `when` over a seam enum is
// exhaustive with no `else`, and the words are `:brand`'s (no second copy).
class WearQuestionsStructuralTest {

    private fun source(name: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see client/android/build.gradle.kts)")
        val file = File(root, "client/android/wear/src/main/kotlin/net/twinion/hummingbird/wear/questions/$name")
        check(file.isFile) { "$name not found under $root" }
        return file.readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
    }

    private val viewModelSrc by lazy { source("QuestionsViewModel.kt") }
    private val screenSrc by lazy { source("QuestionsScreen.kt") }
    private val expandedSrc by lazy { source("WearPaneExpanded.kt") }

    @Test
    fun `nothing on the watch orders the pane list`() {
        for ((name, src) in listOf("QuestionsViewModel.kt" to viewModelSrc, "QuestionsScreen.kt" to screenSrc)) {
            assertFalse("$name must not order a pane list — the seam's order is the order", src.contains("sortedBy") || src.contains("sortedWith"))
        }
        assertTrue("the rank is used as returned", viewModelSrc.contains("Loaded(rankPanesFn(nowMs, facts), nowMs)"))
    }

    @Test
    fun `the cards read decided facts and return no band of their own`() {
        assertFalse(Regex(""":\s*MobilePaneBand\s*[={]""").containsMatchIn(expandedSrc))
        assertTrue(expandedSrc.contains("pane.answer.answerState"))
        assertFalse("no card reads its own clock", expandedSrc.contains("System.currentTimeMillis()"))
    }

    @Test
    fun `every when is exhaustive with no wildcard arm`() {
        for ((name, src) in listOf("QuestionsViewModel.kt" to viewModelSrc, "QuestionsScreen.kt" to screenSrc, "WearPaneExpanded.kt" to expandedSrc)) {
            assertFalse("$name: the exhaustive when is the drift gate", Regex("""(?m)^\s*else\s*->""").containsMatchIn(src))
        }
        for (arm in listOf("Homework", "Scps", "Waste", "Race", "Weekend", "Vacation")) {
            assertTrue("WearPaneExpanded must answer the $arm arm", expandedSrc.contains("is MobilePaneFacts.$arm ->"))
        }
    }

    @Test
    fun `the words are the brand's, the labels the roster's`() {
        assertTrue(expandedSrc.contains("import net.twinion.hummingbird.ui.panes.staleWords"))
        assertTrue(screenSrc.contains("import net.twinion.hummingbird.ui.panes.paneHeadline"))
        assertTrue(viewModelSrc.contains("import uniffi.hummingbird_ffi_mobile.questionRoster"))
        assertFalse("no literal question label on the watch", viewModelSrc.contains("\"Bin collection\""))
    }
}
