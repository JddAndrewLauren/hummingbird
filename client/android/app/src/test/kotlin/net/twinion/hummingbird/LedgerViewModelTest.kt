package net.twinion.hummingbird

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileLedgerRowRecord
import uniffi.hummingbird_ffi_mobile.MobileLedgerRowState

// `LedgerViewModel`'s refresh/complete control flow, with fakes only — the
// same "no generated JNI binding involved" reasoning `AlertsViewModelTest`
// states. That the production wiring really reaches
// `MobileTaskHost.ledgerRows`/`.act`, and that neither this class nor its
// screen re-derives ordering, state or the mark-done gate, is
// `hummingbird-ffi-mobile`'s own `ledger_rows_are_pre_ordered_and_pre_
// gated_for_mark_done` test's job.
class LedgerViewModelTest {

    @Test
    fun `refresh loads whatever the injected fetch fn returns, in its own order`() = runBlocking {
        val vm = LedgerViewModel(
            fetchRowsFn = { listOf(row("b"), row("a")) },
            completeFn = { _, _ -> },
        )

        vm.refresh(1_000)

        assertEquals(listOf("b", "a"), vm.rows.value.map { it.id })
        assertTrue("loading must settle false once the fetch returns", !vm.loading.value)
    }

    @Test
    fun `complete calls the injected complete fn and then reloads`() = runBlocking {
        val calls = mutableListOf<String>()
        val vm = LedgerViewModel(
            fetchRowsFn = { calls += "fetch"; emptyList() },
            completeFn = { itemId, _ -> calls += "complete:$itemId" },
        )

        vm.complete("item-1", 2_000)

        assertEquals(listOf("complete:item-1", "fetch"), calls)
    }

    @Test
    fun `a failed complete reports it and leaves the row readable`() = runBlocking {
        val vm = LedgerViewModel(
            fetchRowsFn = { listOf(row("a")) },
            completeFn = { _, _ -> throw RuntimeException("no network") },
        )

        vm.complete("a", 1_000)

        assertTrue(
            "a failed complete must say so",
            vm.statusLine.value?.contains("Couldn't mark that done") == true,
        )
        assertEquals(listOf("a"), vm.rows.value.map { it.id })
    }

    @Test
    fun `a failed read reports it rather than throwing out of the screen`() = runBlocking {
        val vm = LedgerViewModel(
            fetchRowsFn = { throw RuntimeException("mirror unreadable") },
            completeFn = { _, _ -> },
        )

        vm.refresh(1_000)

        assertTrue(
            "a failed read must say so",
            vm.statusLine.value?.contains("Couldn't read the Ledger") == true,
        )
        assertTrue("loading must settle even on failure", !vm.loading.value)
    }

    @Test
    fun `a successful refresh clears a previous failure line`() = runBlocking {
        var fail = true
        val vm = LedgerViewModel(
            fetchRowsFn = { if (fail) throw RuntimeException("boom") else emptyList() },
            completeFn = { _, _ -> },
        )

        vm.refresh(1_000)
        fail = false
        vm.refresh(1_000)

        assertNull(vm.statusLine.value)
    }
}

internal fun row(
    id: String,
    lastTouchedMs: Long = 1_000,
    pending: Boolean = false,
    canMarkDone: Boolean = true,
    state: MobileLedgerRowState = MobileLedgerRowState.Live,
) = MobileLedgerRowRecord(
    id = id,
    title = "item $id",
    stage = "ready",
    state = state,
    lastTouchedMs = lastTouchedMs,
    pending = pending,
    deadLettered = false,
    hasLiveAlert = false,
    canMarkDone = canMarkDone,
)
