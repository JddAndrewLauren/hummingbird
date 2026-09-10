package net.twinion.hummingbird.wear.items

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The Items list's gates, `WearQuestionsStructuralTest`'s rules re-applied:
// the list is the seam's order (nothing sorts), the rows read a decided band
// (nothing returns one, no row reads its own clock), every `when` over a
// seam enum is exhaustive with no `else`, and the watch only reads — the one
// affordance hands the item to the phone through `ItemLink`, never through
// a second spelling of the URI.
class WearItemsStructuralTest {

    private fun source(name: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see client/android/build.gradle.kts)")
        val file = File(root, "client/android/wear/src/main/kotlin/net/twinion/hummingbird/wear/items/$name")
        check(file.isFile) { "$name not found under $root" }
        return file.readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
    }

    private val viewModelSrc by lazy { source("ItemsViewModel.kt") }
    private val screenSrc by lazy { source("ItemsScreen.kt") }
    private val labelSrc by lazy { source("ItemRowLabel.kt") }

    @Test
    fun `nothing on the watch orders the item list`() {
        for ((name, src) in listOf("ItemsViewModel.kt" to viewModelSrc, "ItemsScreen.kt" to screenSrc)) {
            assertFalse("$name must not order the rows — the board's order is the order", src.contains("sortedBy") || src.contains("sortedWith"))
        }
        assertTrue("the board is flattened as returned", viewModelSrc.contains("board.columns.flatMap { it.items }"))
        assertTrue("the axis is the core's Urgency axis", viewModelSrc.contains("MobileFrontierAxis.URGENCY"))
    }

    @Test
    fun `the rows read a decided band and no clock of their own`() {
        assertFalse(Regex(""":\s*MobileUrgencyBand\s*[={]""").containsMatchIn(labelSrc))
        assertFalse("no row reads its own clock", labelSrc.contains("System.currentTimeMillis()") || labelSrc.contains("LocalDate.now("))
        for ((name, src) in listOf("ItemsViewModel.kt" to viewModelSrc, "ItemsScreen.kt" to screenSrc, "ItemRowLabel.kt" to labelSrc)) {
            assertFalse("$name: the exhaustive when is the drift gate", Regex("""(?m)^\s*else\s*->""").containsMatchIn(src))
        }
    }

    @Test
    fun `the watch reads, and hands the item to the phone through ItemLink`() {
        for (door in listOf(".act(", ".ackAlert(", ".triageItem(", ".setScheduledDate(", ".moveItemToColumn(")) {
            assertFalse("the watch must not write through $door (ADR-0039)", viewModelSrc.contains(door) || screenSrc.contains(door))
        }
        assertTrue(screenSrc.contains("import net.twinion.hummingbird.core.ItemLink"))
        assertTrue(screenSrc.contains("ItemLink.intent("))
        assertFalse("the URI is spelled once, in ItemLink", screenSrc.contains("hummingbird://"))
    }
}
