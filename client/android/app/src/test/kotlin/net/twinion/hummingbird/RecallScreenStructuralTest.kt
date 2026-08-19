package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The M4/#542 counterpart of `RulesScreenStructuralTest`: `Core::search`
// (via `MobileTaskHost.search`) does every bit of Recall's matching,
// grouping and ordering core-side (ADR-0025) — this gate refuses a Kotlin
// copy of any of it. A hand-rolled `sortedBy` or `groupBy` here would
// compile, run, and look right on every fixture anyone would think to
// write; only a source gate catches it, which is why this test reads the
// files.
class RecallScreenStructuralTest {

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }

    /** The file's *code*, with comments removed — `RulesScreenStructuralTest`'s
     * own reasoning: a doc comment must be free to name the thing it
     * forbids without tripping the gate that forbids it. */
    private fun source(name: String) =
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/$name")
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")

    private val screenSrc by lazy { source("RecallScreen.kt") }
    private val viewModelSrc by lazy { source("RecallViewModel.kt") }

    private val both by lazy {
        listOf(
            "RecallScreen.kt" to screenSrc,
            "RecallViewModel.kt" to viewModelSrc,
        )
    }

    @Test
    fun `no recall surface sorts, groups or ranks a result set`() {
        // `Core::search` returns rows already matched, grouped
        // (live/done/archived) and ordered (most recently touched first
        // within a group); `total` is its own un-capped count. Neither
        // file may re-derive any of that.
        for ((name, src) in both) {
            for (spelling in listOf(
                "sortedBy", "sortWith", "sortedWith", ".sorted(",
                "groupBy", "compareBy", "Comparator",
            )) {
                assertFalse(
                    "$name must not order or group a result set — the seam already did ($spelling)",
                    src.contains(spelling),
                )
            }
        }
    }

    @Test
    fun `no recall surface filters or matches a query itself`() {
        // Matching (the multi-word AND, the handle lookup) is
        // `hummingbird_core::search::search`'s alone. A `.filter`/`.contains`
        // scan here over titles or descriptions would be a second, silently
        // drifting matcher.
        for ((name, src) in both) {
            assertFalse(
                "$name must not filter a row list itself",
                Regex("""rows\s*\.\s*filter""").containsMatchIn(src),
            )
            assertFalse(
                "$name must not test a title/description against the query itself",
                Regex("""\.(title|description)\s*\.\s*contains""").containsMatchIn(src),
            )
        }
    }

    @Test
    fun `total is read from the seam, never re-derived from the row list`() {
        assertTrue(
            "RecallScreen must read the seam's own total",
            screenSrc.contains("total"),
        )
        for ((name, src) in both) {
            assertFalse(
                "$name must not stand in rows.size for the seam's total",
                Regex("""total\s*=\s*rows\.size""").containsMatchIn(src),
            )
        }
    }

    @Test
    fun `every when over MobileRecallGroup is exhaustive with no wildcard arm`() {
        val arm = Regex("""MobileRecallGroup\.[A-Z_]+\s*->""")
        assertTrue(
            "RecallScreen.kt must map MobileRecallGroup by its variants",
            arm.containsMatchIn(screenSrc),
        )
        assertFalse(
            "RecallScreen.kt must not fall back to a wildcard arm over MobileRecallGroup",
            Regex("""when\s*\([^)]*group[^)]*\)\s*\{[^}]*else\s*->""", RegexOption.DOT_MATCHES_ALL)
                .containsMatchIn(screenSrc),
        )
    }

    @Test
    fun `only a live row opens item detail`() {
        assertTrue(
            "RecallScreen.kt must gate onOpenItem on the LIVE group",
            screenSrc.contains("MobileRecallGroup.LIVE"),
        )
        assertTrue(
            "RecallScreen.kt must wire onOpenItem",
            screenSrc.contains("onOpenItem"),
        )
    }

    @Test
    fun `the search door reaches MobileTaskHost search, not a second read`() {
        assertTrue(
            "RecallViewModel.kt must reach MobileTaskHost.search",
            viewModelSrc.contains(".search("),
        )
    }

    @Test
    fun `the recall route is registered, reachable, and opens item detail`() {
        val main = source("MainActivity.kt")
        assertTrue(
            "MainActivity must register composable(Routes.RECALL)",
            main.contains("composable(Routes.RECALL)"),
        )
        val recallBlock = main.substringAfter("composable(Routes.RECALL) {").substringBefore("\n            }")
        assertTrue(
            "the Recall route must wire onOpenItem to Routes.itemDetail",
            recallBlock.contains("Routes.itemDetail(itemId)"),
        )
    }
}
