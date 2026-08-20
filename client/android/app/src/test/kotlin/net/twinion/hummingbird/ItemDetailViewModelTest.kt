package net.twinion.hummingbird

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.CaptureFormMeta
import uniffi.hummingbird_ffi_mobile.FieldPatch
import uniffi.hummingbird_ffi_mobile.ItemDetailRecord
import uniffi.hummingbird_ffi_mobile.ItemEdit
import uniffi.hummingbird_ffi_mobile.MetaProblems

// `ItemDetailViewModel`'s load/draft/submit control flow, with fakes only.
// Three things here are worth more than the rest: the draft must survive
// everything that is not an explicit discard (it is human-authored
// content), a submit must carry only the fields actually touched, and
// dirtiness must mean "a human changed something" — never "a sync landed".
class ItemDetailViewModelTest {

    private fun vm(
        fetch: suspend (String, Long) -> ItemDetailRecord? = { id, _ -> itemDetail(id) },
        act: suspend (String, String, Long) -> Unit = { _, _, _ -> },
        ack: suspend (String, Long) -> Unit = { _, _ -> },
        edit: suspend (String, ItemEdit, Long) -> Unit = { _, _, _ -> },
        promote: suspend (String, ItemEdit, Long) -> Unit = { _, _, _ -> },
        sync: suspend () -> Unit = { },
        hasGrillDraft: suspend (String) -> Boolean = { false },
    ) = ItemDetailViewModel(
        fetchFn = fetch,
        actFn = act,
        ackFn = ack,
        editFn = edit,
        promoteFn = promote,
        syncFn = sync,
        hasGrillDraftFn = hasGrillDraft,
        // Stand-ins for the two core rules: no native library exists in a
        // plain JVM process, so the real bindings cannot be called here.
        // `CaptureSubmitRefusalTest` is what proves production wires the
        // real ones — the two gates are complementary, as they are for
        // capture.
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
    fun `an item already in the mirror loads with no sync at all`() = runBlocking {
        val calls = mutableListOf<String>()
        val model = vm(
            fetch = { id, _ -> calls += "fetch"; itemDetail(id) },
            sync = { calls += "sync" },
        )

        model.load("i-1", 1_000)

        assertEquals(listOf("fetch"), calls)
        assertEquals("i-1", (model.state.value as ItemDetailState.Loaded).record.id)
    }

    /** The deep-link race, item-side: a tap can beat the cycle the push
     * enqueued, so a miss syncs once and asks again. */
    @Test
    fun `a missing item syncs once and re-reads, in that order`() = runBlocking {
        val calls = mutableListOf<String>()
        var attempts = 0
        val model = vm(
            fetch = { id, _ ->
                calls += "fetch"
                if (attempts++ == 0) null else itemDetail(id)
            },
            sync = { calls += "sync" },
        )

        model.load("i-1", 1_000)

        assertEquals(listOf("fetch", "sync", "fetch"), calls)
        assertTrue(model.state.value is ItemDetailState.Loaded)
    }

    @Test
    fun `still missing after the sync is an honest state, not a crash`() = runBlocking {
        val model = vm(fetch = { _, _ -> null })

        model.load("i-1", 1_000)

        assertEquals(ItemDetailState.NotSynced, model.state.value)
    }

    /** Recall's rule (#478) reached through this door: the core says an
     * archived item is not editable, and no write leaves here for one —
     * enforced, not merely un-rendered. */
    @Test
    fun `an archived item takes no write, whichever submit is called`() = runBlocking {
        var sent: ItemEdit? = null
        val model = vm(
            fetch = { id, _ -> itemDetail(id, isEditable = false, isArchived = true) },
            edit = { _, edit, _ -> sent = edit },
            promote = { _, edit, _ -> sent = edit },
        )
        model.load("i-1", 1_000)
        model.updateDraft(model.draft.value!!.copy(title = "renamed"))

        model.save("i-1", 2_000)
        model.promote("i-1", 2_000)

        assertNull("history is readable, not editable", sent)
        assertTrue(model.statusLine.value?.contains("readable, not editable") == true)
    }

    /** There is no edit mode to enter: the panel edits every section in
     * place, so the draft exists from the first successful load. */
    @Test
    fun `the draft is seeded by the load itself`() = runBlocking {
        val model = vm()

        model.load("i-1", 1_000)

        assertEquals("item i-1", model.draft.value?.title)
        assertFalse("merely opening an item is not an edit", model.isDirty)
        model.updateDraft(model.draft.value!!.copy(title = "changed"))
        assertTrue(model.isDirty)
    }

    /** The reload the 60-second cadence triggers must not erase what the
     * human is typing — the same silent loss the ViewModel exists to
     * prevent. */
    @Test
    fun `a reload while editing leaves the draft alone`() = runBlocking {
        var reads = 0
        val model = vm(fetch = { id, _ -> itemDetail(id, title = "read ${reads++}") })
        model.load("i-1", 1_000)
        model.updateDraft(model.draft.value!!.copy(title = "half-typed"))

        model.load("i-1", 2_000)

        assertEquals("half-typed", model.draft.value?.title)
        assertTrue("and it is still worth asking about", model.isDirty)
    }

    /** The other half of that rule, and the reason the seed is stored
     * rather than re-derived: a sync landing an edit made on another device
     * changes the record under an untouched draft. That must show through
     * *and* must not invent dirtiness — a Back guard that fires over an
     * edit nobody made is the failure this pins. */
    @Test
    fun `a change landing under a clean draft shows through without faking dirtiness`() = runBlocking {
        var reads = 0
        val model = vm(fetch = { id, _ -> itemDetail(id, title = if (reads++ == 0) "first" else "elsewhere") })
        model.load("i-1", 1_000)
        assertEquals("first", model.draft.value?.title)

        model.load("i-1", 2_000)

        assertEquals("elsewhere", model.draft.value?.title)
        assertFalse("nobody typed anything — do not ask", model.isDirty)
    }

    /** Discarding is a reset to the seed, not a return to a read mode
     * there no longer is. */
    @Test
    fun `discarding puts the seeded values back`() = runBlocking {
        val model = vm()
        model.load("i-1", 1_000)
        model.updateDraft(model.draft.value!!.copy(title = "half-typed"))

        model.discardDraft()

        assertEquals("item i-1", model.draft.value?.title)
        assertFalse(model.isDirty)
    }

    /** Only what was touched rides on the patch: an untouched field is
     * absent from the wire body, so two devices editing different fields
     * of one item do not overwrite each other. */
    @Test
    fun `a save patches only the fields that changed`() = runBlocking {
        var sent: ItemEdit? = null
        val model = vm(edit = { _, edit, _ -> sent = edit })
        model.load("i-1", 1_000)
        model.updateDraft(model.draft.value!!.copy(title = "renamed"))

        model.save("i-1", 2_000)

        assertEquals("renamed", sent?.title)
        assertNull("priority was not touched", sent?.priority)
        assertEquals(FieldPatch.Untouched, sent?.description)
        assertEquals(FieldPatch.Untouched, sent?.deadline)
        assertFalse("the sent draft is the new seed", model.isDirty)
    }

    /** Emptying a field is an edit, not silence: it must reach the wire as
     * an explicit clear, or "this deadline is now gone" is unsayable. */
    @Test
    fun `an emptied field is cleared, not left untouched`() = runBlocking {
        var sent: ItemEdit? = null
        val model = vm(edit = { _, edit, _ -> sent = edit })
        model.load("i-1", 1_000)
        model.updateDraft(model.draft.value!!.copy(deadline = "", size = ""))

        model.save("i-1", 2_000)

        assertEquals(FieldPatch.Clear, sent?.deadline)
        assertEquals(FieldPatch.Clear, sent?.size)
        assertEquals(FieldPatch.Untouched, sent?.context)
    }

    @Test
    fun `a failed save keeps the draft where it can be retried`() = runBlocking {
        val model = vm(edit = { _, _, _ -> throw RuntimeException("offline") })
        model.load("i-1", 1_000)
        model.updateDraft(model.draft.value!!.copy(title = "renamed"))

        model.save("i-1", 2_000)

        assertEquals("renamed", model.draft.value?.title)
        assertTrue(model.statusLine.value?.contains("Couldn't save") == true)
    }

    @Test
    fun `acking from the item screen acks the alert and re-reads`() = runBlocking {
        val calls = mutableListOf<String>()
        val model = vm(
            fetch = { id, _ -> calls += "fetch"; itemDetail(id) },
            ack = { alertId, _ -> calls += "ack:$alertId" },
        )
        model.load("i-1", 1_000)

        model.ack("i-1", "al-9", 2_000)

        assertEquals(listOf("fetch", "ack:al-9", "fetch"), calls)
    }

    /** The act vocabulary crosses unchanged; whether it also acks the
     * alert is the core's decision, not this ViewModel's. */
    @Test
    fun `an act passes the wire word straight through and re-reads`() = runBlocking {
        val calls = mutableListOf<String>()
        val model = vm(
            fetch = { id, _ -> calls += "fetch"; itemDetail(id) },
            act = { _, action, _ -> calls += "act:$action" },
        )
        model.load("i-1", 1_000)

        model.act("i-1", "complete", 2_000)

        assertEquals(listOf("fetch", "act:complete", "fetch"), calls)
    }

    /** The gap this closed: `title` is `NOT NULL`, so an emptied one used
     * to be dropped from the patch and the save reported success having
     * changed nothing — a silent no-op is worse than a refusal. */
    @Test
    fun `a blank title refuses the save instead of silently dropping it`() = runBlocking {
        var sent: ItemEdit? = null
        val model = vm(edit = { _, edit, _ -> sent = edit })
        model.load("i-1", 1_000)
        model.updateDraft(model.draft.value!!.copy(title = ""))

        assertFalse(model.canSave)
        model.save("i-1", 2_000)

        assertNull("nothing may reach the queue", sent)
        assertNotNull("the draft is kept", model.draft.value)
        assertTrue(
            "the refusal must say so",
            model.statusLine.value?.contains("can't be saved") == true,
        )
    }

    /** A malformed date is refused with the core's own words rather than
     * sent for the authority to 400 into the dead-letter journal. */
    @Test
    fun `a malformed date refuses the save and names the problem`() = runBlocking {
        var sent: ItemEdit? = null
        val model = vm(edit = { _, edit, _ -> sent = edit })
        model.load("i-1", 1_000)
        model.updateDraft(model.draft.value!!.copy(deadline = "next tuesday"))

        assertFalse(model.canSave)
        assertEquals("Use YYYY-MM-DD or YYYY-MM-DDTHH:MM", model.metaProblems?.deadline)
        model.save("i-1", 2_000)

        assertNull("nothing may reach the queue", sent)
    }

    /** Clearing a date is not a malformed date: an empty field is a real
     * edit and must stay saveable. */
    @Test
    fun `an emptied date is saveable, not a problem`() = runBlocking {
        val model = vm()
        model.load("i-1", 1_000)
        model.updateDraft(model.draft.value!!.copy(deadline = ""))

        assertTrue(model.canSave)
        assertNull(model.metaProblems?.deadline)
    }

    /** The Triage host's submit, moved here with the draft it sends: the
     * promoting write is a different seam call, not a flag on `edit`, and
     * `promoteToReady = true` is #360's guarantee — pinned structurally at
     * the factory as well, since a fake cannot see the literal. */
    @Test
    fun `promoting sends the touched edits through the promote call`() = runBlocking {
        var sentEdit: ItemEdit? = null
        var sentToEdit: ItemEdit? = null
        val model = vm(
            edit = { _, edit, _ -> sentToEdit = edit },
            promote = { _, edit, _ -> sentEdit = edit },
        )
        model.load("i-1", 1_000)
        model.updateDraft(model.draft.value!!.copy(title = "buy oat milk"))

        model.promote("i-1", 2_000)

        assertEquals("buy oat milk", sentEdit?.title)
        assertNull("a promote is never an edit_item", sentToEdit)
        assertFalse(model.isDirty)
    }

    /** The same field-by-field discipline the save gets: promoting an
     * untouched draft touches nothing. */
    @Test
    fun `promoting patches only the fields that changed`() = runBlocking {
        var sent: ItemEdit? = null
        val model = vm(promote = { _, edit, _ -> sent = edit })
        model.load("i-1", 1_000)

        model.promote("i-1", 2_000)

        assertEquals(FieldPatch.Untouched, sent?.description)
        assertEquals(FieldPatch.Untouched, sent?.deadline)
        assertNull(sent?.title)
    }

    @Test
    fun `a blank title refuses the promote instead of silently dropping it`() = runBlocking {
        var sent: ItemEdit? = null
        val model = vm(promote = { _, edit, _ -> sent = edit })
        model.load("i-1", 1_000)
        model.updateDraft(model.draft.value!!.copy(title = ""))

        model.promote("i-1", 2_000)

        assertNull("nothing may reach the queue", sent)
        assertTrue(
            "the refusal must say so, in the promoting surface's own words",
            model.statusLine.value?.contains("can't be promoted") == true,
        )
    }

    @Test
    fun `a failed promote keeps the draft where it can be retried`() = runBlocking {
        val model = vm(promote = { _, _, _ -> throw RuntimeException("offline") })
        model.load("i-1", 1_000)
        model.updateDraft(model.draft.value!!.copy(title = "renamed"))

        model.promote("i-1", 2_000)

        assertEquals("renamed", model.draft.value?.title)
        assertTrue("still worth asking about on Back", model.isDirty)
        assertTrue(model.statusLine.value?.contains("Couldn't promote") == true)
    }

    /** Nothing is trimmed on the caller's behalf (#110's "raw string
     * reaches the mutation unmodified"), and what counts as empty is the
     * injected rule's answer, never Kotlin's. */
    @Test
    fun `what the human typed is what is sent`() = runBlocking {
        var sent: ItemEdit? = null
        val model = vm(edit = { _, edit, _ -> sent = edit })
        model.load("i-1", 1_000)
        model.updateDraft(model.draft.value!!.copy(context = "@errands "))

        model.save("i-1", 2_000)

        assertEquals(FieldPatch.Set("@errands "), sent?.context)
    }
}


/** An [ItemDetailRecord] in the shape the seam hands over — every verdict
 * is a set field, exactly as Kotlin receives it. */
internal fun itemDetail(
    id: String,
    isEditable: Boolean = true,
    isArchived: Boolean = false,
    canMarkDone: Boolean = true,
    title: String = "item $id",
) = ItemDetailRecord(
    id = id,
    seq = 42,
    title = title,
    description = null,
    stage = "ready",
    size = "quick",
    energy = null,
    context = "@computer",
    agent = false,
    priority = 2,
    projectId = null,
    projectName = null,
    deadline = "2026-08-20",
    scheduledDate = null,
    sourceUrl = null,
    updatedAt = 1_000,
    version = 3,
    steps = emptyList(),
    openBlockers = emptyList(),
    liveAlert = null,
    isArchived = isArchived,
    isEditable = isEditable,
    availableActions = listOf("start", "complete"),
    canMarkDone = canMarkDone,
    microtaskAffordance = null,
)