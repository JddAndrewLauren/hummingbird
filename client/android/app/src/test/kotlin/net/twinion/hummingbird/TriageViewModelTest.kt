package net.twinion.hummingbird

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.CaptureFormMeta
import uniffi.hummingbird_ffi_mobile.FieldPatch
import uniffi.hummingbird_ffi_mobile.ItemEdit
import uniffi.hummingbird_ffi_mobile.MetaProblems
import uniffi.hummingbird_ffi_mobile.TriageBoardRecord
import uniffi.hummingbird_ffi_mobile.MobileUrgencyBand
import uniffi.hummingbird_ffi_mobile.TriageItemRecord

// `TriageViewModel`'s load/select/promote/complete control flow, with fakes
// only — the same house pattern `ItemDetailViewModelTest` uses. Two things
// here are worth more than the rest: only one row is ever open, and
// promoting to Ready is the only save path this screen offers (there is no
// `saveEdits`/"save without promoting" method to test the *absence* of).
class TriageViewModelTest {

    private fun vm(
        fetch: suspend (String) -> TriageBoardRecord = { _ -> triageBoardFixture() },
        triage: suspend (String, Boolean, ItemEdit, Long) -> Unit = { _, _, _, _ -> },
        complete: suspend (String, Long) -> Unit = { _, _ -> },
    ) = TriageViewModel(
        fetchFn = fetch,
        triageFn = triage,
        completeFn = complete,
        // Stand-ins for the core rules: no native library exists in a
        // plain JVM process — `CaptureSubmitRefusalTest` proves production
        // wires the real ones.
        hasContentFn = { it.isNotEmpty() },
        metaProblemsFn = { deadline, scheduledDate ->
            MetaProblems(
                deadline = "Use YYYY-MM-DD or YYYY-MM-DDTHH:MM"
                    .takeIf { deadline.isNotEmpty() && !deadline.startsWith("2026-") },
                scheduledDate = "Use YYYY-MM-DD"
                    .takeIf { scheduledDate.isNotEmpty() && !scheduledDate.startsWith("2026-") },
            )
        },
        formMetaFn = {
            CaptureFormMeta(
                sizes = emptyList(),
                energies = emptyList(),
                suggestedContexts = listOf("@computer", "@errands"),
            )
        },
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
    fun `selecting a row opens exactly it, seeded from the record`() = runBlocking {
        val model = vm(fetch = { triageBoardFixture(items = listOf(triageItemFixture("i-1", title = "buy milk"))) })
        model.load("2026-08-15T12:00")

        model.select("i-1")

        assertEquals("i-1", model.selectedId.value)
        assertEquals("buy milk", model.draft.value?.title)
    }

    @Test
    fun `selecting the open row again closes it`() = runBlocking {
        val model = vm(fetch = { triageBoardFixture(items = listOf(triageItemFixture("i-1"))) })
        model.load("2026-08-15T12:00")
        model.select("i-1")

        model.select("i-1")

        assertNull(model.selectedId.value)
        assertNull(model.draft.value)
    }

    /** Only one row is ever open — opening a second replaces the first's
     * draft entirely, the same "selection, not accumulation" contract the
     * web screen's `selectedId` carries. */
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
        assertEquals("second", model.draft.value?.title)
    }

    /** Promoting is the only destination: it sends `promoteToReady = true`,
     * carries whatever the draft touched, and closes the row on success. */
    @Test
    fun `promoting sends promoteToReady true with the touched edits`() = runBlocking {
        var sentPromote: Boolean? = null
        var sentEdit: ItemEdit? = null
        val model = vm(
            fetch = { triageBoardFixture(items = listOf(triageItemFixture("i-1", title = "buy milk"))) },
            triage = { _, promote, edit, _ -> sentPromote = promote; sentEdit = edit },
        )
        model.load("2026-08-15T12:00")
        model.select("i-1")
        model.updateDraft(model.draft.value!!.copy(title = "buy oat milk"))

        model.promote("i-1", "2026-08-15T12:00", 2_000)

        assertEquals(true, sentPromote)
        assertEquals("buy oat milk", sentEdit?.title)
        assertNull("the row closes on success", model.selectedId.value)
        assertNull(model.draft.value)
    }

    /** Only touched fields ride on the patch — the same field-by-field
     * discipline `ItemDetailViewModelTest` pins for `edit_item`. */
    @Test
    fun `promoting patches only the fields that changed`() = runBlocking {
        var sentEdit: ItemEdit? = null
        val model = vm(
            fetch = { triageBoardFixture(items = listOf(triageItemFixture("i-1"))) },
            triage = { _, _, edit, _ -> sentEdit = edit },
        )
        model.load("2026-08-15T12:00")
        model.select("i-1")

        model.promote("i-1", "2026-08-15T12:00", 2_000)

        assertEquals(FieldPatch.Untouched, sentEdit?.description)
        assertEquals(FieldPatch.Untouched, sentEdit?.deadline)
    }

    @Test
    fun `a blank title refuses to promote instead of silently dropping it`() = runBlocking {
        var sent: ItemEdit? = null
        val model = vm(
            fetch = { triageBoardFixture(items = listOf(triageItemFixture("i-1"))) },
            triage = { _, _, edit, _ -> sent = edit },
        )
        model.load("2026-08-15T12:00")
        model.select("i-1")
        model.updateDraft(model.draft.value!!.copy(title = ""))

        assertFalse(model.canSave)
        model.promote("i-1", "2026-08-15T12:00", 2_000)

        assertNull("nothing may reach the queue", sent)
        assertEquals("i-1", model.selectedId.value)
        assertTrue(model.statusLine.value?.contains("can't be promoted") == true)
    }

    @Test
    fun `a failed promote keeps the row open and the draft where it can be retried`() = runBlocking {
        val model = vm(
            fetch = { triageBoardFixture(items = listOf(triageItemFixture("i-1"))) },
            triage = { _, _, _, _ -> throw RuntimeException("offline") },
        )
        model.load("2026-08-15T12:00")
        model.select("i-1")
        model.updateDraft(model.draft.value!!.copy(title = "renamed"))

        model.promote("i-1", "2026-08-15T12:00", 2_000)

        assertEquals("i-1", model.selectedId.value)
        assertEquals("renamed", model.draft.value?.title)
        assertTrue(model.statusLine.value?.contains("Couldn't promote") == true)
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
        assertNull(model.draft.value)
    }

    /** The failure half of that contract: a failed act leaves the row on
     * the board, so its editor (and draft) stays open where the status line
     * can be read against it and the act retried. */
    @Test
    fun `a failed complete keeps the open row and its draft, and says so`() = runBlocking {
        val model = vm(
            fetch = { triageBoardFixture(items = listOf(triageItemFixture("i-1"))) },
            complete = { _, _ -> throw RuntimeException("offline") },
        )
        model.load("2026-08-15T12:00")
        model.select("i-1")

        model.complete("i-1", "2026-08-15T12:00", 2_000)

        assertEquals("i-1", model.selectedId.value)
        assertEquals("item i-1", model.draft.value?.title)
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

    @Test
    fun `a reload while a row is open leaves its draft alone`() = runBlocking {
        val model = vm(fetch = { triageBoardFixture(items = listOf(triageItemFixture("i-1", title = "buy milk"))) })
        model.load("2026-08-15T12:00")
        model.select("i-1")
        model.updateDraft(model.draft.value!!.copy(title = "half-typed"))

        model.load("2026-08-15T12:00")

        assertEquals("half-typed", model.draft.value?.title)
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
