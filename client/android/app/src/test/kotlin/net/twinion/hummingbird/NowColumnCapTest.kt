package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileUrgencyBand
import uniffi.hummingbird_ffi_mobile.NowBlockedEntryRecord
import uniffi.hummingbird_ffi_mobile.NowBoardRecord
import uniffi.hummingbird_ffi_mobile.NowColumnRecord
import uniffi.hummingbird_ffi_mobile.NowItemRecord

// `NowScreen`'s two pure board-reading decisions, which used to be inline
// lambdas inside the LazyColumn and so were testable only through a
// composition the module has no emulator for:
//
// - `cappedColumnRows` — the six-card column cap and its one exception, the
//   open pane's own item (the pane lives in that row's slot now, so a
//   re-rank past the cap would make it vanish under a live selection).
// - `selectedPaneIsEmitted` — whether the pane is in the list at all, which
//   is what the dirty-Back branch turns on.
//
// Behavioural, not structural: these are ordinary functions over the
// already-decided board record, so nothing here needs Robolectric.
class NowColumnCapTest {

    // `NowViewModelTest`'s own record helper, same shape: only `id` matters
    // to either function under test.
    private fun record(id: String) = NowItemRecord(
        id = id,
        title = "item $id",
        deadline = null,
        urgency = MobileUrgencyBand.CALM,
        priority = 0L,
        context = null,
        size = null,
        energy = null,
        availableActions = listOf("start"),
        stage = "ready",
        canMarkDone = true,
    )

    private fun items(count: Int) = (1..count).map { record("i$it") }

    private fun ids(rows: List<NowItemRecord>) = rows.map { it.id }

    @Test
    fun `the cap holds while nothing is selected`() {
        val column = items(9)
        assertEquals(
            listOf("i1", "i2", "i3", "i4", "i5", "i6"),
            ids(cappedColumnRows(column, selectedId = null)),
        )
        // And the "N more" count the screen derives from it stays honest.
        assertEquals(3, column.size - cappedColumnRows(column, null).size)
    }

    @Test
    fun `a short column is drawn whole`() {
        assertEquals(listOf("i1", "i2"), ids(cappedColumnRows(items(2), selectedId = null)))
        assertEquals(listOf("i1", "i2"), ids(cappedColumnRows(items(2), selectedId = "i2")))
    }

    @Test
    fun `a selection inside the cap changes nothing`() {
        val column = items(9)
        assertEquals(
            ids(cappedColumnRows(column, selectedId = null)),
            ids(cappedColumnRows(column, selectedId = "i3")),
        )
    }

    @Test
    fun `a selection past the cap is drawn too, in rank order`() {
        val column = items(9)
        val visible = cappedColumnRows(column, selectedId = "i8")
        assertEquals(
            "the capped six plus the open item, at its own rank — never promoted to the top",
            listOf("i1", "i2", "i3", "i4", "i5", "i6", "i8"),
            ids(visible),
        )
        // The exception adds, never replaces: the count the "N more" button
        // shows drops by exactly the one row now on screen.
        assertEquals(2, column.size - visible.size)
    }

    @Test
    fun `a selection in another column is not smuggled in`() {
        val column = items(9)
        assertEquals(
            ids(cappedColumnRows(column, selectedId = null)),
            ids(cappedColumnRows(column, selectedId = "elsewhere")),
        )
    }

    private fun board(
        columns: List<NowColumnRecord>,
        blocked: List<NowBlockedEntryRecord> = emptyList(),
    ) = NowBoardRecord(
        columns = columns,
        blocked = blocked,
        contexts = emptyList(),
        liveColumnKeys = columns.map { it.value ?: "" },
        shownCount = 0u,
        totalCount = 0u,
    )

    @Test
    fun `the pane is emitted while its column is open`() {
        val board = board(listOf(NowColumnRecord(value = "home", label = "Home", items = items(3))))
        assertTrue(selectedPaneIsEmitted(board, collapsed = emptySet(), selectedId = "i2"))
        assertTrue(
            "another column's collapse is not this one's",
            selectedPaneIsEmitted(board, collapsed = setOf("errand"), selectedId = "i2"),
        )
    }

    @Test
    fun `collapsing the column takes the pane out of the list`() {
        val board = board(listOf(NowColumnRecord(value = "home", label = "Home", items = items(3))))
        assertFalse(selectedPaneIsEmitted(board, collapsed = setOf("home"), selectedId = "i2"))
    }

    @Test
    fun `the no-value column collapses under the empty key`() {
        // `NowScreen` keys that column `column.value ?: ""` — the same
        // string `toggleCollapsed` stores, so this function must read it the
        // same way or the one unnamed column can never be seen as shut.
        val board = board(listOf(NowColumnRecord(value = null, label = null, items = items(3))))
        assertTrue(selectedPaneIsEmitted(board, collapsed = emptySet(), selectedId = "i1"))
        assertFalse(selectedPaneIsEmitted(board, collapsed = setOf(""), selectedId = "i1"))
    }

    @Test
    fun `a board that no longer carries the item has no pane`() {
        // The facet path: the selection survives a filter that drops its row.
        val board = board(listOf(NowColumnRecord(value = "home", label = "Home", items = items(3))))
        assertFalse(selectedPaneIsEmitted(board, collapsed = emptySet(), selectedId = "gone"))
        assertFalse(selectedPaneIsEmitted(board, collapsed = emptySet(), selectedId = null))
        assertFalse(selectedPaneIsEmitted(null, collapsed = emptySet(), selectedId = "i1"))
    }

    @Test
    fun `a blocked row's pane is always emitted`() {
        // The Blocked section has no collapse of its own, so no collapse set
        // can hide it -- including one that happens to hold its item's id.
        val board = board(
            columns = emptyList(),
            blocked = listOf(
                NowBlockedEntryRecord(item = record("b1"), blockedByTitles = listOf("the blocker")),
            ),
        )
        assertTrue(selectedPaneIsEmitted(board, collapsed = setOf("b1", ""), selectedId = "b1"))
    }

    // ---- source pins -----------------------------------------------------

    /** The raw file, comments intact — the pin below is about a comment's
     * placement, so it cannot use `NowItemDoorTest`'s stripping reader. */
    private fun rawSource(name: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set -- run under Gradle (see app/build.gradle.kts)")
        val file = File(root, "client/android/app/src/main/kotlin/net/twinion/hummingbird/$name")
        check(file.isFile) { "$name not found under $root" }
        return file.readText()
    }

    @Test
    fun `ColumnHeader's KDoc is still ColumnHeader's`() {
        // The inline-expansion slice inserted `selectedItemKey` and
        // `SelectedItemCard` BETWEEN this KDoc and the function it documents,
        // which reads as documentation of the wrong declaration -- a false
        // comment, which this repo treats as a defect rather than a nit.
        val src = rawSource("NowScreen.kt")
        assertTrue(
            "the chevron KDoc must sit immediately above ColumnHeader",
            Regex(
                """ic_chevron_down\][\s\S]{0,600}?\*/\s*@Composable\s*internal fun ColumnHeader\(""",
            ).containsMatchIn(src),
        )
    }
}
