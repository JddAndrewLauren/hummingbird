package net.twinion.hummingbird

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.AlertRecord

// `AlertsViewModel`'s refresh/ack control flow, with fakes only — the same
// "no generated JNI binding involved" reasoning `NowViewModelTest` states.
// That the production wiring really reaches `MobileTaskHost.alerts`/
// `.ackAlert`, and that neither this class nor its screen re-derives
// liveness or ordering, is `AlertsScreenStructuralTest`'s job.
class AlertsViewModelTest {

    @Test
    fun `refresh loads whatever the injected fetch fn returns, in its own order`() = runBlocking {
        // Core sorts (`raised_at` desc, id tiebreak). Whatever order it
        // hands over is the order that renders; nothing here re-sorts.
        val vm = AlertsViewModel(
            fetchAlertsFn = { listOf(alert("b"), alert("a")) },
            ackFn = { _, _ -> },
        )

        vm.refresh(1_000)

        assertEquals(listOf("b", "a"), vm.alerts.value.map { it.id })
        assertTrue("loading must settle false once the fetch returns", !vm.loading.value)
    }

    @Test
    fun `refresh passes the given clock straight through`() = runBlocking {
        var seenNowMs: Long? = null
        val vm = AlertsViewModel(
            fetchAlertsFn = { nowMs -> seenNowMs = nowMs; emptyList() },
            ackFn = { _, _ -> },
        )

        vm.refresh(1_723_000_000_000)

        assertEquals(1_723_000_000_000, seenNowMs)
    }

    @Test
    fun `ack calls the injected ack fn and then reloads`() = runBlocking {
        val calls = mutableListOf<String>()
        val vm = AlertsViewModel(
            fetchAlertsFn = { calls += "fetch"; emptyList() },
            ackFn = { alertId, _ -> calls += "ack:$alertId" },
        )

        vm.ack("alert-1", 2_000)

        // Reload after the mutation is what makes `canAck` and the row's
        // departure from the live list show up immediately.
        assertEquals(listOf("ack:alert-1", "fetch"), calls)
    }

    @Test
    fun `a failed ack reports it and leaves the list readable`() = runBlocking {
        val vm = AlertsViewModel(
            fetchAlertsFn = { listOf(alert("a")) },
            ackFn = { _, _ -> throw RuntimeException("no network") },
        )

        vm.ack("a", 1_000)

        assertTrue(
            "a failed ack must say so",
            vm.statusLine.value?.contains("Couldn't ack") == true,
        )
        // Still there, still offered — the row is the durable retry.
        assertEquals(listOf("a"), vm.alerts.value.map { it.id })
    }

    @Test
    fun `a failed read reports it rather than throwing out of the screen`() = runBlocking {
        val vm = AlertsViewModel(
            fetchAlertsFn = { throw RuntimeException("mirror unreadable") },
            ackFn = { _, _ -> },
        )

        vm.refresh(1_000)

        assertTrue(
            "a failed read must say so",
            vm.statusLine.value?.contains("Couldn't read alerts") == true,
        )
        assertTrue("loading must settle even on failure", !vm.loading.value)
    }

    @Test
    fun `a successful refresh clears a previous failure line`() = runBlocking {
        var fail = true
        val vm = AlertsViewModel(
            fetchAlertsFn = { if (fail) throw RuntimeException("boom") else emptyList() },
            ackFn = { _, _ -> },
        )

        vm.refresh(1_000)
        fail = false
        vm.refresh(1_000)

        assertNull(vm.statusLine.value)
    }
}

/** An [AlertRecord] in the shape the seam hands over — `isLive`/`canAck`
 * are decided fields, set here rather than computed, exactly as Kotlin
 * receives them. */
internal fun alert(
    id: String,
    canAck: Boolean = true,
    isLive: Boolean = true,
    severity: String? = "urgent",
    body: String? = "the detail line",
    url: String? = null,
) = AlertRecord(
    id = id,
    source = "fly",
    sourceKey = "hb-worker",
    subjectKey = null,
    title = "alert $id",
    body = body,
    url = url,
    severity = severity,
    raisedAt = 1_000,
    resolvedAt = null,
    dismissedAt = null,
    expiresAt = null,
    version = 1,
    isLive = isLive,
    canAck = canAck,
)
