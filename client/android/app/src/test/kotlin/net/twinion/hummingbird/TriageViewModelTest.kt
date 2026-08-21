package net.twinion.hummingbird

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.TriageBoardRecord
import uniffi.hummingbird_ffi_mobile.MobileUrgencyBand
import uniffi.hummingbird_ffi_mobile.TriageItemRecord

// `TriageViewModel`'s load/select/complete control flow, with fakes only —
// the same house pattern `ItemDetailViewModelTest` uses. What this ViewModel
// owns since the panel unification is the board and the selection: the
// draft, the promote and their refusals moved to `ItemDetailViewModel`,
// where that file's own tests now hold them. Only one row is ever open, and
// a mark-done closes it.
class TriageViewModelTest {

    private fun vm(
        fetch: suspend (String) -> TriageBoardRecord = { _ -> triageBoardFixture() },
        complete: suspend (String, Long) -> Unit = { _, _ -> },
    ) = TriageViewModel(
        fetchFn = fetch,
        completeFn = complete,
    )

    @Test
    fun `loading reads the whole board`() = runBlocking {
        val model = vm(fetch = { triageBoardFixture(capturedCount = 2, grillingCount = 1) })

        model.load("2026-08-15T12:00")

        val loaded = model.state.value as TriageState.Loaded
        assertEquals(2u, loaded.board.capturedCount)
        assertEquals(1u, loaded.board.grillingCount)
    }

    @Test
    fun `selecting a row opens exactly it`() = runBlocking {
        val model = vm(fetch = { triageBoardFixture(items = listOf(triageItemFixture("i-1", title = "buy milk"))) })
        model.load("2026-08-15T12:00")

        model.select("i-1")

        assertEquals("i-1", model.selectedId.value)
    }

    @Test
    fun `selecting the open row again closes it`() = runBlocking {
        val model = vm(fetch = { triageBoardFixture(items = listOf(triageItemFixture("i-1"))) })
        model.load("2026-08-15T12:00")
        model.select("i-1")

        model.select("i-1")

        assertNull(model.selectedId.value)
    }

    /** Only one row is ever open — opening a second replaces the first,
     * the same "selection, not accumulation" contract the web screen's
     * `selectedId` carries. */
    @Test
    fun `opening a different row replaces whichever was open`() = runBlocking {
        val model = vm(
            fetch = {
                triageBoardFixture(items = listOf(triageItemFixture("i-1", title = "first"), triageItemFixture("i-2", title = "second")))
            },
        )
        model.load("2026-08-15T12:00")
        model.select("i-1")

        model.select("i-2")

        assertEquals("i-2", model.selectedId.value)
    }

    /** A selection is only ever a row this ViewModel is holding — the
     * panel is rendered off the board, so an id that is not on it would
     * open a pane with nothing behind it. */
    @Test
    fun `an id that is not on the board cannot be selected`() = runBlocking {
        val model = vm(fetch = { triageBoardFixture(items = listOf(triageItemFixture("i-1"))) })
        model.load("2026-08-15T12:00")

        model.select("i-nowhere")

        assertNull(model.selectedId.value)
    }

    /** What a promote (or a mark-done from the opened pane) needs: the item
     * leaves this queue, so the selection must go with it or it dangles at
     * a vanished row. */
    @Test
    fun `closing the selection shuts whichever row is open`() = runBlocking {
        val model = vm(fetch = { triageBoardFixture(items = listOf(triageItemFixture("i-1"))) })
        model.load("2026-08-15T12:00")
        model.select("i-1")

        model.closeSelection()

        assertNull(model.selectedId.value)
    }

    /** The checkmark: `Core::act`'s `complete`, never a triage — and it
     * closes an open row on the item it completed, since the row it just
     * completed no longer belongs on the board. */
    @Test
    fun `completing an open row calls act complete and closes it`() = runBlocking {
        var completed: String? = null
        val model = vm(
            fetch = { triageBoardFixture(items = listOf(triageItemFixture("i-1"))) },
            complete = { itemId, _ -> completed = itemId },
        )
        model.load("2026-08-15T12:00")
        model.select("i-1")

        model.complete("i-1", "2026-08-15T12:00", 2_000)

        assertEquals("i-1", completed)
        assertNull(model.selectedId.value)
    }

    /** The failure half of that contract: a failed act leaves the row on
     * the board, so its pane stays open where the status line can be read
     * against it and the act retried. */
    @Test
    fun `a failed complete keeps the open row, and says so`() = runBlocking {
        val model = vm(
            fetch = { triageBoardFixture(items = listOf(triageItemFixture("i-1"))) },
            complete = { _, _ -> throw RuntimeException("offline") },
        )
        model.load("2026-08-15T12:00")
        model.select("i-1")

        model.complete("i-1", "2026-08-15T12:00", 2_000)

        assertEquals("i-1", model.selectedId.value)
        assertTrue(model.statusLine.value?.contains("Couldn't complete") == true)
    }

    /** Cancellation is not a failure: it rethrows, and no "Couldn't
     * complete" is worded for a coroutine that was simply cancelled. */
    @Test
    fun `a cancelled complete rethrows rather than reading as a failure`() = runBlocking {
        val model = vm(
            fetch = { triageBoardFixture(items = listOf(triageItemFixture("i-1"))) },
            complete = { _, _ -> throw CancellationException("scope left") },
        )
        model.load("2026-08-15T12:00")

        try {
            model.complete("i-1", "2026-08-15T12:00", 2_000)
            fail("complete must rethrow cancellation")
        } catch (expected: CancellationException) {
        }

        assertNull("cancellation must never be worded as a failure", model.statusLine.value)
    }
}

private fun triageBoardFixture(
    items: List<TriageItemRecord> = emptyList(),
    capturedCount: Int = items.count { it.stage == "triage" },
    grillingCount: Int = items.count { it.stage == "grilling" },
) = TriageBoardRecord(
    items = items,
    capturedCount = capturedCount.toUInt(),
    grillingCount = grillingCount.toUInt(),
)

private fun triageItemFixture(
    id: String,
    title: String = "item $id",
    stage: String = "triage",
    canMarkDone: Boolean = true,
) = TriageItemRecord(
    id = id,
    title = title,
    description = null,
    stage = stage,
    size = null,
    energy = null,
    context = null,
    priority = 0,
    projectId = null,
    deadline = null,
    urgency = MobileUrgencyBand.CALM,
    scheduledDate = null,
    source = null,
    createdAt = 0,
    canMarkDone = canMarkDone,
    canGrill = true,
    hasGrillDraft = false,
)
