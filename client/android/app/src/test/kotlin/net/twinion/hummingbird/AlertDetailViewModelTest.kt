package net.twinion.hummingbird

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// `AlertDetailViewModel`'s load/ack control flow, with fakes only. The case
// that matters is the deep-link race: a notification tap can beat the sync
// the push enqueued, so `MobileTaskHost.alert` returns null for a row this
// device has not synced. It is deliberately not liveness-filtered, so null
// means exactly that and nothing else.
class AlertDetailViewModelTest {

    @Test
    fun `a row already in the mirror loads with no sync at all`() = runBlocking {
        val calls = mutableListOf<String>()
        val vm = AlertDetailViewModel(
            fetchAlertFn = { id, _ -> calls += "fetch"; alert(id) },
            ackFn = { _, _ -> },
            syncFn = { calls += "sync" },
        )

        vm.load("a-1", 1_000)

        assertEquals(listOf("fetch"), calls)
        assertEquals("a-1", (vm.state.value as AlertDetailState.Loaded).record.id)
    }

    @Test
    fun `a missing row syncs once and re-reads, in that order`() = runBlocking {
        val calls = mutableListOf<String>()
        var attempts = 0
        val vm = AlertDetailViewModel(
            fetchAlertFn = { id, _ ->
                calls += "fetch"
                if (attempts++ == 0) null else alert(id)
            },
            ackFn = { _, _ -> },
            syncFn = { calls += "sync" },
        )

        vm.load("a-1", 1_000)

        assertEquals(listOf("fetch", "sync", "fetch"), calls)
        assertTrue(vm.state.value is AlertDetailState.Loaded)
    }

    @Test
    fun `still missing after the sync is an honest state, not a crash`() = runBlocking {
        val calls = mutableListOf<String>()
        val vm = AlertDetailViewModel(
            fetchAlertFn = { _, _ -> calls += "fetch"; null },
            ackFn = { _, _ -> },
            syncFn = { calls += "sync" },
        )

        vm.load("a-1", 1_000)

        assertEquals(AlertDetailState.NotSynced, vm.state.value)
        // Exactly one sync — the screen offers a manual retry rather than
        // looping on a condition a loop cannot resolve.
        assertEquals(listOf("fetch", "sync", "fetch"), calls)
    }

    @Test
    fun `the screen survives its own ack -- the record keeps rendering`() = runBlocking {
        // `alert()` is not liveness-filtered, so acking does not make the
        // row unreadable; it comes back with canAck false.
        var acked = false
        val vm = AlertDetailViewModel(
            fetchAlertFn = { id, _ -> alert(id, canAck = !acked) },
            ackFn = { _, _ -> acked = true },
            syncFn = { },
        )

        vm.load("a-1", 1_000)
        assertTrue((vm.state.value as AlertDetailState.Loaded).record.canAck)

        vm.ack("a-1", 2_000)

        val after = vm.state.value as AlertDetailState.Loaded
        assertEquals("a-1", after.record.id)
        assertTrue("the acked record must still render, just settled", !after.record.canAck)
    }

    @Test
    fun `a failed ack reports it and still re-reads`() = runBlocking {
        val calls = mutableListOf<String>()
        val vm = AlertDetailViewModel(
            fetchAlertFn = { id, _ -> calls += "fetch"; alert(id) },
            ackFn = { _, _ -> throw RuntimeException("no network") },
            syncFn = { },
        )

        vm.ack("a-1", 1_000)

        assertTrue(
            "a failed ack must say so",
            vm.statusLine.value?.contains("Couldn't ack") == true,
        )
        assertEquals(listOf("fetch"), calls)
    }

    @Test
    fun `a throwing read lands on the not-synced state with a line, never an exception`() =
        runBlocking {
            val vm = AlertDetailViewModel(
                fetchAlertFn = { _, _ -> throw RuntimeException("mirror unreadable") },
                ackFn = { _, _ -> },
                syncFn = { },
            )

            vm.load("a-1", 1_000)

            assertEquals(AlertDetailState.NotSynced, vm.state.value)
            assertTrue(
                "a failed read must say so",
                vm.statusLine.value?.contains("Couldn't read this alert") == true,
            )
        }

    @Test
    fun `the alert id and clock reach the fetch fn unchanged`() = runBlocking {
        var seenId: String? = null
        var seenNowMs: Long? = null
        val vm = AlertDetailViewModel(
            fetchAlertFn = { id, nowMs -> seenId = id; seenNowMs = nowMs; alert(id) },
            ackFn = { _, _ -> },
            syncFn = { },
        )

        vm.load("a-xyz", 1_723_000_000_000)

        assertEquals("a-xyz", seenId)
        assertEquals(1_723_000_000_000, seenNowMs)
    }
}
