package net.twinion.hummingbird

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
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
    fun `a cancellation from a keystroke landing mid-crossing propagates, never reported as a failure`() = runBlocking {
        val vm = RecallViewModel(searchFn = { _, _ -> throw CancellationException("keystroke superseded this call") })

        vm.setQueryText("stamps")
        var caught: CancellationException? = null
        try {
            vm.search(1_000)
        } catch (error: CancellationException) {
            caught = error
        }

        assertTrue("the cancellation must propagate rather than be swallowed", caught != null)
        assertNull("a cancelled search must never flash a failure line", vm.statusLine.value)
    }

    @Test
    fun `tapping a row opens it, tapping it again closes it`() = runBlocking {
        val vm = RecallViewModel(searchFn = { _, _ -> outcome() })

        vm.select("i-1")
        assertEquals("i-1", vm.selectedId.value)

        vm.select("i-1")
        assertNull("the open row's own tap must close it", vm.selectedId.value)
    }

    @Test
    fun `tapping a different row moves the expansion rather than adding one`() = runBlocking {
        val vm = RecallViewModel(searchFn = { _, _ -> outcome() })

        vm.select("i-1")
        vm.select("i-2")

        assertEquals("i-2", vm.selectedId.value)
    }

    @Test
    fun `a keystroke closes the open panel`() = runBlocking {
        // The panel must not stand open under a query it no longer answers
        // — this class's own header states the rule, and the web's
        // `useRecallWiring.ts` clears its whole slot for it.
        val vm = RecallViewModel(searchFn = { _, _ -> outcome() })
        vm.select("i-1")

        vm.setQueryText("stamps")

        assertNull(vm.selectedId.value)
    }

    @Test
    fun `the open-gesture reset is a method the host calls, not composition state`() {
        // A1a: `AppRoot.openRecall` calls this at the gesture that opens the
        // overlay. It lives here — and not in a `LaunchedEffect(Unit)` in
        // the overlay — precisely so that an Activity recreation (a
        // fold/unfold on the install target) re-enters the composition
        // WITHOUT collapsing an open, possibly mid-edit, panel. Nothing in
        // this process can recreate an Activity, so what a unit test can
        // hold is the half that makes the fix possible: the selection
        // survives everything except an explicit clear.
        val vm = RecallViewModel(searchFn = { _, _ -> outcome() })
        vm.select("i-1")

        assertEquals(
            "only an explicit clear may close the panel",
            "i-1",
            vm.selectedId.value,
        )

        vm.clearSelection()

        assertNull(vm.selectedId.value)
    }

    @Test
    fun `a superseded search never lowers the flag under the one replacing it`() = runBlocking {
        // A3b: `LaunchedEffect(query)` cancels the previous search without
        // joining it, so the cancelled call's `finally` can land AFTER its
        // replacement has already raised `loading` — leaving a search in
        // flight with no spinner under it. Staged here with two gates: the
        // superseded call is held mid-crossing until the replacing one is
        // also mid-crossing, then released to fail.
        val supersededGate = CompletableDeferred<Unit>()
        val replacingGate = CompletableDeferred<Unit>()
        var calls = 0
        val vm = RecallViewModel(
            searchFn = { _, _ ->
                if (++calls == 1) {
                    supersededGate.await()
                    throw CancellationException("keystroke superseded this call")
                }
                replacingGate.await()
                outcome(row("a"))
            },
        )

        vm.setQueryText("stamps")
        val superseded = launch { try { vm.search(1_000) } catch (expected: CancellationException) {} }
        val replacing = launch { vm.search(2_000) }
        yield()

        supersededGate.complete(Unit)
        superseded.join()

        assertTrue(
            "the replacing search is still crossing — its flag must survive the cancelled one's finally",
            vm.loading.value,
        )

        replacingGate.complete(Unit)
        replacing.join()

        assertTrue("the newest search's answer stands", vm.rows.value.map { it.id } == listOf("a"))
        assertTrue("and it settles its own flag", !vm.loading.value)
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
