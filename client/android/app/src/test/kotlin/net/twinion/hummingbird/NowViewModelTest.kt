package net.twinion.hummingbird

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileUrgencyBand
import uniffi.hummingbird_ffi_mobile.NowItemRecord

// NowViewModel's refresh/act control flow, exercised entirely with fakes —
// the same "no generated JNI binding involved" reasoning
// CaptureViewModelTest states for its own class. The production wiring's
// own correctness — that [NowViewModel.create] really reaches
// `MobileTaskHost.nowQueue`/`.act` and that nothing on this screen
// re-derives ordering, urgency or the act vocabulary locally — is
// NowScreenStructuralTest's job.
class NowViewModelTest {

    private fun record(id: String, actions: List<String> = listOf("start")) = NowItemRecord(
        id = id,
        title = "item $id",
        deadline = null,
        urgency = MobileUrgencyBand.CALM,
        priority = 0L,
        context = null,
        availableActions = actions,
    )

    @Test
    fun `refresh loads whatever the injected fetch fn returns, in its own order`() = runBlocking {
        val vm = NowViewModel(
            fetchQueueFn = { listOf(record("b"), record("a")) },
            actFn = { _, _, _ -> },
        )

        vm.refresh("2026-08-15T12:00")

        assertEquals(listOf("b", "a"), vm.items.value.map { it.id })
        assertTrue("loading must settle false once the fetch returns", !vm.loading.value)
    }

    @Test
    fun `refresh passes the given deadline-shaped now straight through`() = runBlocking {
        var seenNow: String? = null
        val vm = NowViewModel(
            fetchQueueFn = { now -> seenNow = now; emptyList() },
            actFn = { _, _, _ -> },
        )

        vm.refresh("2026-08-15T09:30")

        assertEquals("2026-08-15T09:30", seenNow)
    }

    @Test
    fun `act calls the injected act fn with the given item, action and clock`() = runBlocking {
        var seenItemId: String? = null
        var seenAction: String? = null
        var seenNowMs: Long? = null
        val vm = NowViewModel(
            fetchQueueFn = { emptyList() },
            actFn = { itemId, action, nowMs ->
                seenItemId = itemId
                seenAction = action
                seenNowMs = nowMs
            },
        )

        vm.act("item-1", "complete", 5_000L, "2026-08-15T12:00")

        assertEquals("item-1", seenItemId)
        assertEquals("complete", seenAction)
        assertEquals(5_000L, seenNowMs)
    }

    @Test
    fun `act reloads the queue after acting, reflecting the mutation immediately`() = runBlocking {
        var actHasRun = false
        val vm = NowViewModel(
            fetchQueueFn = { if (actHasRun) listOf(record("still-here")) else listOf(record("gone"), record("still-here")) },
            actFn = { _, _, _ -> actHasRun = true },
        )
        vm.refresh("2026-08-15T12:00")
        assertEquals(listOf("gone", "still-here"), vm.items.value.map { it.id })

        vm.act("gone", "complete", 1_000L, "2026-08-15T12:00")

        assertEquals(listOf("still-here"), vm.items.value.map { it.id })
    }
}
