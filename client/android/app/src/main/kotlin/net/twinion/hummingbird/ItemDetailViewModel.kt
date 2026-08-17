package net.twinion.hummingbird

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlin.random.Random
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.twinion.hummingbird.core.CoreHolder
import net.twinion.hummingbird.sync.SyncWorker
import uniffi.hummingbird_ffi_mobile.FieldPatch
import uniffi.hummingbird_ffi_mobile.ItemDetailRecord
import uniffi.hummingbird_ffi_mobile.ItemEdit

/** What the item screen is showing — the three states
 * [AlertDetailState] has, for the same reason: "this device has not synced
 * that item yet" is a real, reachable, temporary condition on a deep link,
 * and it is neither a loaded record nor an error. */
sealed interface ItemDetailState {
    data object Loading : ItemDetailState

    data class Loaded(val record: ItemDetailRecord) : ItemDetailState

    data object NotSynced : ItemDetailState
}

/** One in-progress edit: every editable field as the text or vocabulary
 * word the form holds.
 *
 * Strings throughout, including the numeric priority, because that is what
 * a text field and a choice row actually hold — the conversion to the
 * seam's typed patch happens once, in [ItemDraft.toEdit], where the
 * three-way Untouched/Clear/Set decision is made against the record the
 * draft started from.
 *
 * `projectId` and the **Delegation axis** are deliberately absent: neither
 * is editable from this screen (the seam's `edit_item` cannot touch the
 * axis at all), and a draft field with nothing to set it from would be a
 * silent no-op the reader could not tell from a working one.
 */
data class ItemDraft(
    val title: String,
    val description: String,
    val context: String,
    val deadline: String,
    val scheduledDate: String,
    val size: String,
    val energy: String,
    val priority: String,
) {
    companion object {
        fun of(record: ItemDetailRecord) = ItemDraft(
            title = record.title,
            description = record.description.orEmpty(),
            context = record.context.orEmpty(),
            deadline = record.deadline.orEmpty(),
            scheduledDate = record.scheduledDate.orEmpty(),
            size = record.size.orEmpty(),
            energy = record.energy.orEmpty(),
            priority = record.priority.toString(),
        )
    }

    /** The draft as a seam patch, against the record it started from.
     *
     * The three-way decision lives here and nowhere else. A field the
     * human did not touch stays [FieldPatch.Untouched] — absent from the
     * wire body entirely — so two devices editing different fields of one
     * item do not overwrite each other's work. A field emptied is
     * [FieldPatch.Clear], an explicit null, because "this deadline is now
     * gone" is a real edit and not the same as silence.
     *
     * `title` and `priority` cannot be cleared (`NOT NULL` columns), so a
     * blank title or an unparseable priority is left untouched rather than
     * sent for the authority to refuse. The form does not offer either.
     */
    fun toEdit(from: ItemDetailRecord): ItemEdit = ItemEdit(
        title = title.takeIf { it.isNotBlank() && it != from.title },
        priority = priority.toLongOrNull()?.takeIf { it != from.priority },
        description = patch(description, from.description),
        size = patch(size, from.size),
        energy = patch(energy, from.energy),
        context = patch(context, from.context),
        // Not editable from this screen — see the class doc.
        projectId = FieldPatch.Untouched,
        deadline = patch(deadline, from.deadline),
        scheduledDate = patch(scheduledDate, from.scheduledDate),
    )

    private fun patch(drafted: String, original: String?): FieldPatch {
        val value = drafted.trim().takeIf { it.isNotEmpty() }
        return when {
            value == original -> FieldPatch.Untouched
            value == null -> FieldPatch.Clear
            else -> FieldPatch.Set(value)
        }
    }
}

// The item screen's whole read and its edit mode (#141's last slice,
// ADR-0027), injected-fn shaped like every other ViewModel here.
//
// **The draft lives here, never in a `remember {}`.** That is
// `CaptureViewModel.factory`'s recorded defect (the fold/unfold one): a
// composition-scoped draft is lost when the Activity is recreated, which on
// the Fold happens every time the device is opened or closed. A draft is
// human-authored content, so losing it silently is the one failure this
// screen must not have — the same reasoning the dead-letter journal and
// "parse is additive" already encode elsewhere in this repo.
//
// Acking from here is the same gesture as acking from alert detail
// (ADR-0027 part 3): the alert about an item is reachable where the item
// is, or the tap that rang would have moved the Ack one screen further
// away. Completing or cancelling the item acks it too — but that decision
// belongs to the core, so this ViewModel just calls `act` and re-reads.
class ItemDetailViewModel(
    private val fetchFn: suspend (itemId: String, nowMs: Long) -> ItemDetailRecord?,
    private val actFn: suspend (itemId: String, action: String, nowMs: Long) -> Unit,
    private val ackFn: suspend (alertId: String, nowMs: Long) -> Unit,
    private val editFn: suspend (itemId: String, edit: ItemEdit, nowMs: Long) -> Unit,
    private val syncFn: suspend () -> Unit,
) : ViewModel() {

    private val _state = MutableStateFlow<ItemDetailState>(ItemDetailState.Loading)
    val state: StateFlow<ItemDetailState> = _state.asStateFlow()

    private val _statusLine = MutableStateFlow<String?>(null)
    val statusLine: StateFlow<String?> = _statusLine.asStateFlow()

    /** The edit in progress, or null in read mode. */
    private val _draft = MutableStateFlow<ItemDraft?>(null)
    val draft: StateFlow<ItemDraft?> = _draft.asStateFlow()

    /** Whether the draft differs from the record it started from — what
     * decides whether Back must ask before discarding. An untouched draft
     * is never fought over. */
    val isDirty: Boolean
        get() {
            val current = _draft.value ?: return false
            val record = (_state.value as? ItemDetailState.Loaded)?.record ?: return false
            return current != ItemDraft.of(record)
        }

    /** Loads the item: read, and on a miss sync once and read again.
     *
     * **A reload never disturbs an open draft.** The sync cadence above
     * the NavHost ticks every 60 seconds; rebuilding the draft from the
     * freshly-read record would erase whatever the human had typed in
     * between, which is the same silent loss the ViewModel exists to
     * prevent. */
    suspend fun load(itemId: String, nowMs: Long) {
        if (_draft.value == null) _state.value = ItemDetailState.Loading
        try {
            fetchFn(itemId, nowMs)?.let {
                _state.value = ItemDetailState.Loaded(it)
                _statusLine.value = null
                return
            }
            syncFn()
            val afterSync = fetchFn(itemId, nowMs)
            _state.value = afterSync
                ?.let { ItemDetailState.Loaded(it) }
                ?: ItemDetailState.NotSynced
            _statusLine.value = null
        } catch (error: Exception) {
            _state.value = ItemDetailState.NotSynced
            _statusLine.value = "Couldn't read this item — ${error.message}"
        }
    }

    /** Enters edit mode with the loaded record's own values.
     *
     * Refused on an archived item: `isEditable` is the core's verdict
     * (Recall's rule, #478), and the screen offers no affordance either —
     * this second check is here because a verdict worth rendering is worth
     * enforcing at the one place that acts on it. */
    fun beginEdit() {
        val record = (_state.value as? ItemDetailState.Loaded)?.record ?: return
        if (!record.isEditable) return
        _draft.value = ItemDraft.of(record)
    }

    fun updateDraft(draft: ItemDraft) {
        _draft.value = draft
    }

    /** Leaves edit mode, discarding the draft. Only ever called once the
     * human has said so — see the screen's discard confirmation. */
    fun discardEdit() {
        _draft.value = null
    }

    /** Saves the draft as one CAS patch, then re-reads. The draft is
     * cleared only on success: a failed save leaves the human's words
     * where they can still be seen and retried. */
    suspend fun save(itemId: String, nowMs: Long) {
        val record = (_state.value as? ItemDetailState.Loaded)?.record ?: return
        val draft = _draft.value ?: return
        try {
            editFn(itemId, draft.toEdit(record), nowMs)
            _draft.value = null
        } catch (error: Exception) {
            _statusLine.value = "Couldn't save — ${error.message}"
            return
        }
        load(itemId, nowMs)
    }

    /** One act from the item screen. Completing or cancelling also acks
     * the live alert about this item — decided in the core, not here. */
    suspend fun act(itemId: String, action: String, nowMs: Long) {
        val failure = try {
            actFn(itemId, action, nowMs)
            null
        } catch (error: Exception) {
            "Couldn't $action — ${error.message}"
        }
        load(itemId, nowMs)
        failure?.let { _statusLine.value = it }
    }

    /** Acks the live alert about this item, then re-reads so the card
     * updates in place. */
    suspend fun ack(itemId: String, alertId: String, nowMs: Long) {
        val failure = try {
            ackFn(alertId, nowMs)
            null
        } catch (error: Exception) {
            "Couldn't ack — ${error.message}"
        }
        load(itemId, nowMs)
        failure?.let { _statusLine.value = it }
    }

    companion object {
        fun create(context: Context): ItemDetailViewModel =
            ItemDetailViewModel(
                fetchFn = { itemId, nowMs ->
                    CoreHolder.get(context.applicationContext).itemDetail(itemId, nowMs)
                },
                actFn = { itemId, action, nowMs ->
                    CoreHolder.get(context.applicationContext).act(itemId, action, nowMs)
                },
                ackFn = { alertId, nowMs ->
                    CoreHolder.get(context.applicationContext).ackAlert(alertId, nowMs)
                },
                editFn = { itemId, edit, nowMs ->
                    CoreHolder.get(context.applicationContext).editItem(itemId, edit, nowMs)
                },
                // `"push"`, not `"timer"`: a deep link landing during a
                // backoff window must still be able to fetch its row —
                // `AlertDetailViewModel`'s own note applies verbatim.
                syncFn = {
                    CoreHolder.get(context.applicationContext).run(
                        System.currentTimeMillis(),
                        SyncWorker.TRIGGER_PUSH,
                        false,
                        Random.nextDouble(),
                    )
                },
            )

        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
