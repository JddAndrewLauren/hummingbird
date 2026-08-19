package net.twinion.hummingbird

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileDoneRecord

// `DoneViewModel`'s refresh control flow, with fakes only — the same
// "no generated JNI binding involved" reasoning `AlertsViewModelTest`
// states. That the production wiring really reaches
// `MobileTaskHost.doneItems`, and that the seam (not this class) decided
// the order, is `hummingbird-ffi-mobile`'s own `done_items_orders_most_
// recently_touched_first` test's job.
class DoneViewModelTest {

    @Test
    fun `refresh loads whatever the injected fetch fn returns, in its own order`() = runBlocking {
        val vm = DoneViewModel(fetchDoneFn = { listOf(done("b"), done("a")) })

        vm.refresh()

        assertEquals(listOf("b", "a"), vm.items.value.map { it.id })
        assertTrue("loading must settle false once the fetch returns", !vm.loading.value)
    }

    @Test
    fun `a failed read reports it rather than throwing out of the screen`() = runBlocking {
        val vm = DoneViewModel(fetchDoneFn = { throw RuntimeException("mirror unreadable") })

        vm.refresh()

        assertTrue(
            "a failed read must say so",
            vm.statusLine.value?.contains("Couldn't read Done") == true,
        )
        assertTrue("loading must settle even on failure", !vm.loading.value)
    }

    @Test
    fun `a successful refresh clears a previous failure line`() = runBlocking {
        var fail = true
        val vm = DoneViewModel(fetchDoneFn = { if (fail) throw RuntimeException("boom") else emptyList() })

        vm.refresh()
        fail = false
        vm.refresh()

        assertNull(vm.statusLine.value)
    }
}

internal fun done(id: String, updatedAt: Long = 1_000, pending: Boolean = false) = MobileDoneRecord(
    id = id,
    title = "item $id",
    updatedAt = updatedAt,
    pending = pending,
)
