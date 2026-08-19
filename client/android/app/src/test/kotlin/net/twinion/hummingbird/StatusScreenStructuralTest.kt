package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The Status surface's own gate (#536/M4, #537/M4, ADR-0025): the pane
// shell must re-derive no pane decision. `answerState` and `band` arrive
// already decided, off `hummingbird_core::decisions::panes` through
// `MobileTaskHost.rankPanes`, and the `when`s over their uniffi mirrors are
// the drift gate — a ninth standing question or a sixth band must fail a
// build, never render as a silently-missing row. `MobilePaneBand`'s own
// exhaustive `when` moved to `PaneShell.kt` at #537, when the row/band/
// status-words rendering `StatusScreen.kt` used to own directly became the
// shared shell `NowScreen.kt`'s own three panes render through too — this
// file's gate follows the code, one `screenSrc`/`shellSrc` pair rather than
// one file.
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
    private val shellSrc by lazy { source("PaneShell.kt") }

    @Test
    fun `every when over the seam pane enums is exhaustive with no wildcard arm`() {
        val arm = Regex("""MobileStandingQuestion\.[A-Z_]+\s*->""")
        assertTrue(
            "StatusScreen.kt must map MobileStandingQuestion by its variants",
            arm.containsMatchIn(screenSrc),
        )
        val bandArm = Regex("""MobilePaneBand\.[A-Z_]+\s*->""")
        assertTrue(
            "PaneShell.kt must map MobilePaneBand by its variants",
            bandArm.containsMatchIn(shellSrc),
        )
        for (src in listOf(screenSrc, shellSrc)) {
            assertFalse(
                "no when over a seam pane enum may carry a wildcard arm",
                Regex("""(?m)^\s*else\s*->""").containsMatchIn(src),
            )
        }
    }

    @Test
    fun `the shell bands nothing itself — band and answerState are read, never computed`() {
        assertTrue(
            "the pane row must read the seam's own band",
            shellSrc.contains("pane.answer.band"),
        )
        assertTrue(
            "the pane row must read the seam's own answerState",
            shellSrc.contains("pane.answer.answerState"),
        )
        for (src in listOf(screenSrc, shellSrc)) {
            assertFalse(
                "no local comparator may reorder the seam's own display order",
                src.contains("sortedBy") || src.contains("sortedWith"),
            )
        }
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
