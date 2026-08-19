package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The Status surface's own gate (#536/M4, ADR-0025): the pane shell must
// re-derive no pane decision. `answerState` and `band` arrive already
// decided, off `hummingbird_core::decisions::panes` through
// `MobileTaskHost.rankPanes`, and the two `when`s over their uniffi
// mirrors are the drift gate — a ninth standing question or a sixth band
// must fail this file's build, never render as a silently-missing row.
class StatusScreenStructuralTest {

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

    private val screenSrc by lazy { source("StatusScreen.kt") }

    @Test
    fun `every when over the seam pane enums is exhaustive with no wildcard arm`() {
        for (enum in listOf("MobileStandingQuestion", "MobilePaneBand")) {
            val arm = Regex("""$enum\.[A-Z_]+\s*->""")
            assertTrue(
                "StatusScreen.kt must map $enum by its variants",
                arm.containsMatchIn(screenSrc),
            )
        }
        assertFalse(
            "no when over a seam pane enum may carry a wildcard arm",
            Regex("""(?m)^\s*else\s*->""").containsMatchIn(screenSrc),
        )
    }

    @Test
    fun `the screen bands nothing itself — band and answerState are read, never computed`() {
        assertTrue(
            "the pane list must read the seam's own band",
            screenSrc.contains("pane.answer.band"),
        )
        assertTrue(
            "the pane list must read the seam's own answerState",
            screenSrc.contains("pane.answer.answerState"),
        )
        assertFalse(
            "no local comparator may reorder the seam's own display order",
            screenSrc.contains("sortedBy") || screenSrc.contains("sortedWith"),
        )
    }

    @Test
    fun `ProofScreen is gone and nothing in the app references it`() {
        for (file in listOf("MainActivity.kt", "StatusScreen.kt", "StatusViewModel.kt")) {
            val raw = repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/$file")
            assertFalse(
                "$file must declare no ProofScreen composable",
                raw.contains("fun ProofScreen"),
            )
            assertFalse(
                "$file must not call ProofScreen",
                raw.contains("ProofScreen("),
            )
        }
    }

    @Test
    fun `the status route renders StatusScreen`() {
        val main = source("MainActivity.kt")
        assertTrue(
            "MainActivity must register the status route",
            main.contains("composable(Routes.STATUS)"),
        )
        val open = main.indexOf("composable(Routes.STATUS)")
        val body = main.substring(open, main.indexOf("\n        }", open))
        assertTrue(
            "the status route must render StatusScreen",
            body.contains("StatusScreen("),
        )
    }
}
