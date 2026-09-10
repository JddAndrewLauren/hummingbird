package net.twinion.hummingbird.wear.items

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileUrgencyBand
import uniffi.hummingbird_ffi_mobile.NowBoardRecord
import uniffi.hummingbird_ffi_mobile.NowColumnRecord
import uniffi.hummingbird_ffi_mobile.NowItemRecord

// `ItemsViewModel` over fakes: the rows are the board's columns flattened in
// the board's order and nothing else; Loading is real; one row is open at a
// time and its description is fetched on opening, once.
class ItemsViewModelTest {

    private fun item(id: String, band: MobileUrgencyBand) = NowItemRecord(
        id = id, title = id, deadline = null, urgency = band, priority = 0L,
        context = null, size = null, energy = null, availableActions = emptyList(),
        stage = "ready", canMarkDone = true,
    )

    private fun board(vararg columns: List<NowItemRecord>) = NowBoardRecord(
        columns = columns.map { NowColumnRecord(value = null, label = null, items = it) },
        blocked = emptyList(), contexts = emptyList(), liveColumnKeys = emptyList(),
        shownCount = 0u, totalCount = 0u,
    )

    @Test
    fun `loading is real, then the rows are the columns flattened in the board's order`() = runBlocking {
        // Deliberately not urgency-sorted: the order is the board's to give.
        val vm = ItemsViewModel(
            boardFn = { board(listOf(item("c", MobileUrgencyBand.CALM)), listOf(item("o", MobileUrgencyBand.OVERDUE), item("o2", MobileUrgencyBand.OVERDUE))) },
            descriptionFn = { _, _ -> null },
        )
        assertNull(vm.loaded.value)
        vm.load(nowMs = 1_757_530_000_000L)
        assertEquals(listOf("c", "o", "o2"), vm.loaded.value?.rows?.map { it.id })
        assertEquals(10, vm.loaded.value?.today?.length)
    }

    @Test
    fun `one row is open at a time, its description fetched once, and a second tap folds it`() = runBlocking {
        val fetched = mutableListOf<String>()
        val vm = ItemsViewModel(
            boardFn = { board(emptyList()) },
            descriptionFn = { id, _ -> fetched += id; "about $id" },
        )
        vm.toggle("a", 1L)
        assertEquals(ItemsViewModel.Open("a", "about a", fetched = true), vm.open.value)
        vm.toggle("b", 1L)
        assertEquals("b", vm.open.value?.itemId)
        vm.toggle("b", 1L)
        assertNull(vm.open.value)
        assertEquals(listOf("a", "b"), fetched)
    }

    /** The fold that lands while the description is still on its way must
     * win: a fetch that completes afterwards may not reopen the card. */
    @Test
    fun `a fold during the fetch is not undone when the fetch lands`() = runBlocking {
        val parked = CompletableDeferred<String?>()
        val vm = ItemsViewModel(boardFn = { board(emptyList()) }, descriptionFn = { _, _ -> parked.await() })
        val opening = launch { vm.toggle("a", 1L) }
        // Let the toggle reach the fetch and park there.
        while (vm.open.value == null) yield()
        vm.toggle("a", 1L)
        assertNull(vm.open.value)
        parked.complete("about a")
        opening.join()
        assertNull("the landed fetch must not resurrect the folded card", vm.open.value)
    }
}
