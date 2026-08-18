package net.twinion.hummingbird

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.FieldPatch
import uniffi.hummingbird_ffi_mobile.ItemDetailRecord
import uniffi.hummingbird_ffi_mobile.ItemEdit
import uniffi.hummingbird_ffi_mobile.MetaProblems

// `ItemDetailViewModel`'s load/edit/save control flow, with fakes only.
// Two things here are worth more than the rest: the draft must survive
// everything that is not an explicit discard (it is human-authored
// content), and a save must carry only the fields actually touched.
class ItemDetailViewModelTest {

    private fun vm(
        fetch: suspend (String, Long) -> ItemDetailRecord? = { id, _ -> itemDetail(id) },
        act: suspend (String, String, Long) -> Unit = { _, _, _ -> },
        ack: suspend (String, Long) -> Unit = { _, _ -> },
        edit: suspend (String, ItemEdit, Long) -> Unit = { _, _, _ -> },
        sync: suspend () -> Unit = { },
        hasGrillDraft: suspend (String) -> Boolean = { false },
    ) = ItemDetailViewModel(
        fetchFn = fetch,
        actFn = act,
        ackFn = ack,
        editFn = edit,
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
     * archived item is not editable, and nothing here may open a draft on
     * one anyway. */
    @Test
    fun `an archived item exposes no edit path`() = runBlocking {
        val model = vm(fetch = { id, _ -> itemDetail(id, isEditable = false, isArchived = true) })
        model.load("i-1", 1_000)

        model.beginEdit()

        assertNull(model.draft.value)
    }

    @Test
    fun `an untouched draft is not dirty, so Back is never fought`() = runBlocking {
        val model = vm()
        model.load("i-1", 1_000)

        model.beginEdit()

        assertFalse(model.isDirty)
        model.updateDraft(model.draft.value!!.copy(title = "changed"))
        assertTrue(model.isDirty)
    }

    /** The reload the 60-second cadence triggers must not erase what the
     * human is typing — the same silent loss the ViewModel exists to
     * prevent. */
    @Test
    fun `a reload while editing leaves the draft alone`() = runBlocking {
        val model = vm()
        model.load("i-1", 1_000)
        model.beginEdit()
        model.updateDraft(model.draft.value!!.copy(title = "half-typed"))

        model.load("i-1", 2_000)

        assertEquals("half-typed", model.draft.value?.title)
    }

    /** Only what was touched rides on the patch: an untouched field is
     * absent from the wire body, so two devices editing different fields
     * of one item do not overwrite each other. */
    @Test
    fun `a save patches only the fields that changed`() = runBlocking {
        var sent: ItemEdit? = null
        val model = vm(edit = { _, edit, _ -> sent = edit })
        model.load("i-1", 1_000)
        model.beginEdit()
        model.updateDraft(model.draft.value!!.copy(title = "renamed"))

        model.save("i-1", 2_000)

        assertEquals("renamed", sent?.title)
        assertNull("priority was not touched", sent?.priority)
        assertEquals(FieldPatch.Untouched, sent?.description)
        assertEquals(FieldPatch.Untouched, sent?.deadline)
        assertNull("a saved draft leaves edit mode", model.draft.value)
    }

    /** Emptying a field is an edit, not silence: it must reach the wire as
     * an explicit clear, or "this deadline is now gone" is unsayable. */
    @Test
    fun `an emptied field is cleared, not left untouched`() = runBlocking {
        var sent: ItemEdit? = null
        val model = vm(edit = { _, edit, _ -> sent = edit })
        model.load("i-1", 1_000)
        model.beginEdit()
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
        model.beginEdit()
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
        model.beginEdit()
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
        model.beginEdit()
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
        model.beginEdit()
        model.updateDraft(model.draft.value!!.copy(deadline = ""))

        assertTrue(model.canSave)
        assertNull(model.metaProblems?.deadline)
    }

    /** Nothing is trimmed on the caller's behalf (#110's "raw string
     * reaches the mutation unmodified"), and what counts as empty is the
     * injected rule's answer, never Kotlin's. */
    @Test
    fun `what the human typed is what is sent`() = runBlocking {
        var sent: ItemEdit? = null
        val model = vm(edit = { _, edit, _ -> sent = edit })
        model.load("i-1", 1_000)
        model.beginEdit()
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
) = ItemDetailRecord(
    id = id,
    seq = 42,
    title = "item $id",
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
    microtaskAffordance = null,
)