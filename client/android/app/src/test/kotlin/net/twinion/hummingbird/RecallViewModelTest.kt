package net.twinion.hummingbird

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileRecallGroup
import uniffi.hummingbird_ffi_mobile.MobileRecallOutcome
import uniffi.hummingbird_ffi_mobile.MobileRecallRowRecord

// `RecallViewModel`'s query/search control flow, with fakes only — the same
// "no generated JNI binding involved" reasoning `LedgerViewModelTest`
// states. That the production wiring really reaches
// `MobileTaskHost.search`, and that this class re-derives no matching,
// grouping or ordering of its own, is `hummingbird-ffi-mobile`'s own
// `search_*` tests' job (plus `RecallScreenStructuralTest`'s source gate).
class RecallViewModelTest {

    @Test
    fun `a blank query never reaches the seam and clears any previous rows`() = runBlocking {
        var calls = 0
        val vm = RecallViewModel(searchFn = { _, _ -> calls++; outcome() })

        vm.setQueryText("something")
        vm.search(1_000)
        vm.setQueryText("   ")
        vm.search(2_000)

        assertEquals(1, calls)
        assertTrue(vm.rows.value.isEmpty())
        assertEquals(0u, vm.total.value)
    }

    @Test
    fun `search asks with whatever query was last set, and stores the seam's answer verbatim`() = runBlocking {
        var seenQuery: String? = null
        val vm = RecallViewModel(
            searchFn = { query, _ ->
                seenQuery = query
                outcome(row("b", MobileRecallGroup.DONE), row("a", MobileRecallGroup.LIVE), total = 5u)
            },
        )

        vm.setQueryText("stamps")
        vm.search(1_000)

        assertEquals("stamps", seenQuery)
        // The seam's own order is kept — this class never re-sorts it.
        assertEquals(listOf("b", "a"), vm.rows.value.map { it.id })
        assertEquals(5u, vm.total.value)
        assertTrue("loading must settle false once the answer arrives", !vm.loading.value)
    }

    @Test
    fun `a failed search reports it rather than throwing out of the screen`() = runBlocking {
        val vm = RecallViewModel(searchFn = { _, _ -> throw RuntimeException("mirror unreadable") })

        vm.setQueryText("stamps")
        vm.search(1_000)

        assertTrue(
            "a failed search must say so",
            vm.statusLine.value?.contains("Couldn't search") == true,
        )
        assertTrue("loading must settle even on failure", !vm.loading.value)
    }

    @Test
    fun `a successful search clears a previous failure line`() = runBlocking {
        var fail = true
        val vm = RecallViewModel(
            searchFn = { _, _ -> if (fail) throw RuntimeException("boom") else outcome() },
        )

        vm.setQueryText("stamps")
        vm.search(1_000)
        fail = false
        vm.search(2_000)

        assertNull(vm.statusLine.value)
    }
}

private fun outcome(vararg rows: MobileRecallRowRecord, total: UInt = rows.size.toUInt()) =
    MobileRecallOutcome(rows = rows.toList(), total = total)

private fun row(id: String, group: MobileRecallGroup = MobileRecallGroup.LIVE) =
    MobileRecallRowRecord(
        id = id,
        title = "item $id",
        stage = "ready",
        group = group,
        updatedAt = 1_000L,
        pending = false,
    )
