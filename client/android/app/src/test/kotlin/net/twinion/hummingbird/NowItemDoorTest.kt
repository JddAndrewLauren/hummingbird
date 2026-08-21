package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// #521: item detail used to be reachable only by tapping a notification --
// found on hardware, where the accessibility tree showed 22 clickable nodes
// on Now and not one of them the card. With no alert ringing there was no
// way to open an item at all.
//
// The door's destination changed with the inline-expansion slice: a tapped
// card now opens `ItemDetailPanel` in place, ABOVE the still-standing board
// (the web's `SelectedItemSection`, ADR-0021 decision 7 / #404 -- an early
// return of the panel instead of the frontier was the web's own bug, and
// these pins keep Android off the same path). The full-screen route
// remains the notification's and Recall's door (ADR-0027;
// `NavigationStructuralTest` pins that leg).
//
// Structural, like `NavigationStructuralTest` and for the same reason: the
// module has no emulator in CI, and "the card is clickable" is exactly the
// kind of claim the compiler cannot make. A `Card` without `onClick` still
// compiles and still renders.
class NowItemDoorTest {

    private fun source(name: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set -- run under Gradle (see app/build.gradle.kts)")
        val file = File(root, "client/android/app/src/main/kotlin/net/twinion/hummingbird/$name")
        check(file.isFile) { "$name not found under $root" }
        return file.readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
    }

    @Test
    fun `the Now card is itself the door, and it opens the inline panel`() {
        val src = source("NowScreen.kt")
        assertTrue(
            "the row's Card must take onClick -- a clickable modifier would drop the " +
                "ripple, the Button role and the minimum touch target",
            Regex("""Card\(\s*onClick = onOpen""").containsMatchIn(source("NowRow.kt")),
        )
        assertTrue(
            "tapping a card must drive the ViewModel's selection, not a navigation",
            src.contains("viewModel.selectItem(record.id)"),
        )
        assertTrue(
            "NowScreen must render ItemDetailPanel for the selected item",
            src.contains("ItemDetailPanel("),
        )
    }

    @Test
    fun `the selected card expands in place, and the board keeps rendering around it`() {
        // ADR-0021 decision 7's mechanism, pinned structurally: the panel
        // is an item INSIDE the one LazyColumn and the frontier is never
        // early-returned away by a selection. What changed on 2026-08-20 is
        // WHERE inside: the pane took the selected row's own slot, so the
        // card the operator tapped expands rather than a block appearing at
        // the top of the board. Comments are stripped, so these are code
        // positions.
        val src = source("NowScreen.kt")
        val lazyColumn = src.indexOf("LazyColumn(")
        val boardBranches = src.indexOf("loading && currentBoard == null")
        val paneEmit = src.indexOf("item(key = selectedItemKey(")
        val paneSection = src.indexOf("nowPaneSection(", lazyColumn)
        assertTrue("NowScreen must keep its one LazyColumn", lazyColumn >= 0)
        assertTrue("the board's own branches must sit inside it", boardBranches > lazyColumn)
        assertTrue(
            "the pane must be emitted among the board's rows, not as a block before them",
            paneEmit > boardBranches,
        )
        assertTrue("the now-surface panes must still close the list", paneSection > paneEmit)
        assertFalse(
            "and nothing may scroll the list on a selection — the pane opens where the " +
                "finger already is, and the jump is what made the first tap and the " +
                "second look like different gestures",
            src.contains("animateScrollToItem(0)"),
        )

        // The row-or-pane branch, bounded to the columns loop: every item
        // draws exactly one of the two, so the board keeps one line per
        // item and no item is drawn twice.
        val columnsLoop = src.substring(
            src.indexOf("for (column in currentBoard.columns)"),
            src.indexOf("if (currentBoard.blocked.isNotEmpty())"),
        )
        assertTrue(
            "the selected record must render as the pane in its own slot",
            columnsLoop.replace(Regex("""\s+"""), " ").contains(
                "if (record.id == selectedId) { item(key = selectedItemKey(record.id)) { SelectedItemCard(",
            ),
        )
        assertTrue(
            "and every other record as its row",
            columnsLoop.replace(Regex("""\s+"""), " ").contains(
                "} else { item(key = " + "\"\$key-\${record.id}\"" + ") { NowRow(",
            ),
        )
        assertTrue(
            "the selected item must be rendered even when the column cap would hide it — " +
                "a re-rank must not make the open pane vanish",
            columnsLoop.contains("it.id in cappedIds || it.id == selectedId"),
        )
    }

    @Test
    fun `opening an item from Now navigates nowhere`() {
        // The inline door replaced the `navigate(Routes.itemDetail(...))`
        // hand-off. The route stays reachable -- notifications
        // (`openItemFromNotification`, pinned by NavigationStructuralTest)
        // and Recall still land on it -- but Now's own composable block
        // must not: two doors from one card would mean two copies of the
        // opened item's state.
        val src = source("MainActivity.kt")
        val nowBlock = Regex("""composable\(Routes\.NOW\) \{[\s\S]*?\n {12}\}""").find(src)?.value
            ?: error("MainActivity does not register composable(Routes.NOW)")
        assertFalse(
            "Now's composable block must not navigate to the item route",
            nowBlock.contains("navigate(Routes.itemDetail("),
        )
        assertTrue(
            "Now's one remaining navigation is the Grill takeover",
            nowBlock.contains("navigate(Routes.grill("),
        )
    }
}
