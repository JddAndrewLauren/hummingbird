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
import uniffi.hummingbird_ffi_mobile.CaptureFormMeta
import uniffi.hummingbird_ffi_mobile.FieldPatch
import uniffi.hummingbird_ffi_mobile.ItemDetailRecord
import uniffi.hummingbird_ffi_mobile.ItemEdit
import uniffi.hummingbird_ffi_mobile.MetaProblems
import uniffi.hummingbird_ffi_mobile.canSubmitCapture
import uniffi.hummingbird_ffi_mobile.captureFormMeta
import uniffi.hummingbird_ffi_mobile.captureMetaProblems

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
    /** #782's Link, `""` when unset — the pane draws and edits it. */
    val linkUrl: String = "",
    val linkLabel: String = "",
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
            linkUrl = record.linkUrl.orEmpty(),
            linkLabel = record.linkLabel.orEmpty(),
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
     * **What counts as empty is [hasContent]'s answer, never Kotlin's.**
     * A hand-written blank check is an automatic reject in this repo
     * (M1-5/#503, gated by `CaptureSubmitRefusalTest`, which is why the
     * banned spellings do not appear even in this comment): the standard
     * library's disagrees with the real rule on a pasted BOM, the single
     * likeliest invisible to arrive in a text field. The rule has one
     * owner in
     * `hummingbird_core::decisions::capture`, and it arrives here as a
     * function rather than a copy.
     *
     * Nothing is trimmed on the caller's behalf either — #110's "raw
     * string reaches the mutation unmodified" — so what the human typed is
     * what is sent.
     *
     * `title` and `priority` cannot be cleared (`NOT NULL` columns). A
     * blank title never reaches here: [ItemDetailViewModel.canSave]
     * refuses the save while one is showing, rather than dropping the
     * field silently and reporting success.
     */
    fun toEdit(from: ItemDetailRecord, hasContent: (String) -> Boolean): ItemEdit = ItemEdit(
        title = title.takeIf { hasContent(it) && it != from.title },
        priority = priority.toLongOrNull()?.takeIf { it != from.priority },
        description = patch(description, from.description, hasContent),
        size = patch(size, from.size, hasContent),
        energy = patch(energy, from.energy, hasContent),
        context = patch(context, from.context, hasContent),
        // Not editable from this screen — see the class doc.
        projectId = FieldPatch.Untouched,
        deadline = patch(deadline, from.deadline, hasContent),
        scheduledDate = patch(scheduledDate, from.scheduledDate, hasContent),
        // #782: the seam clears the name with the URL — one row state — so
        // a cleared URL beside an untouched name is a whole clear, not a
        // stranded name.
        linkUrl = patch(linkUrl, from.linkUrl, hasContent),
        linkLabel = patch(linkLabel, from.linkLabel, hasContent),
    )

    private fun patch(
        drafted: String,
        original: String?,
        hasContent: (String) -> Boolean,
    ): FieldPatch {
        val value = drafted.takeIf(hasContent)
        return when {
            value == original -> FieldPatch.Untouched
            value == null -> FieldPatch.Clear
            else -> FieldPatch.Set(value)
        }
    }
}

// The item pane's whole read and its one draft (#141's last slice,
// ADR-0027), injected-fn shaped like every other ViewModel here. Shared by
// all four of `ItemDetailPanel`'s hosts — Now's inline expansion, the
// notification/Recall route, the Recall overlay and Triage — one instance
// per item id, resolved under `key = "item-$itemId"`.
//
// **There is no edit mode.** Every section of the panel edits in place and
// one submit sends the accumulated draft: `save` from three hosts,
// `promote` from Triage. So the draft exists from the first successful
// `load` onwards rather than being begun and discarded.
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
    /** The promoting write, `Core::triage` with `promoteToReady = true`.
     * Separate from [editFn] because it is a different seam call, not a
     * flag on this one — see [promote]. */
    private val promoteFn: suspend (itemId: String, edit: ItemEdit, nowMs: Long) -> Unit,
    private val syncFn: suspend () -> Unit,
    /** #539's "Grill me"/"Resume grill" label source — whether this item
     * already carries a saved Grill draft. */
    private val hasGrillDraftFn: suspend (itemId: String) -> Boolean,
    /** The core's blank rule, injected rather than called directly so a
     * plain JVM test can drive this ViewModel — there is no native library
     * in that process (`CaptureViewModel`'s own doc). The production
     * wiring is the real binding, and `CaptureSubmitRefusalTest` gates
     * that it stays the real one. */
    private val hasContentFn: (String) -> Boolean,
    /** The core's date-field rule, injected for the same reason. */
    private val metaProblemsFn: (deadline: String, scheduledDate: String) -> MetaProblems,
    /** The shared form components' vocabulary door, injected the same way
     * `CaptureViewModel.formMetaFn` is: every size/energy/context word the
     * panel's editors offer comes from here, never a Kotlin literal. */
    private val formMetaFn: () -> CaptureFormMeta,
) : ViewModel() {

    /** Read once, on first use — the vocabulary does not change
     * mid-session, the same laziness `CaptureViewModel.formMeta` uses. */
    val formMeta: CaptureFormMeta by lazy { formMetaFn() }

    private val _state = MutableStateFlow<ItemDetailState>(ItemDetailState.Loading)
    val state: StateFlow<ItemDetailState> = _state.asStateFlow()

    private val _statusLine = MutableStateFlow<String?>(null)
    val statusLine: StateFlow<String?> = _statusLine.asStateFlow()

    /** The edit in progress — non-null from the first successful [load]
     * onwards, because the panel has no read mode to be in: every section
     * edits in place and one submit sends whatever the draft holds. Null
     * only while the item has never loaded. */
    private val _draft = MutableStateFlow<ItemDraft?>(null)
    val draft: StateFlow<ItemDraft?> = _draft.asStateFlow()

    /** What [_draft] was seeded from, kept beside it rather than re-derived
     * from the current record.
     *
     * **This is load-bearing, not a convenience.** Dirtiness must mean "the
     * human changed something", and a background sync landing an edit made
     * on another device changes the record under an untouched draft — if
     * dirtiness were `draft != ItemDraft.of(record)`, that sync would
     * invent a dirty draft out of nothing and start fighting Back over an
     * edit nobody made. The seed only moves when [load] finds the draft
     * clean, or when a submit succeeds. */
    private val _seed = MutableStateFlow<ItemDraft?>(null)

    private val _hasGrillDraft = MutableStateFlow(false)
    val hasGrillDraft: StateFlow<Boolean> = _hasGrillDraft.asStateFlow()

    /** What is wrong with the draft's two free-text dates right now, by
     * field — the core's answer, shown next to the field it belongs to. */
    val metaProblems: MetaProblems?
        get() = _draft.value?.let { metaProblemsFn(it.deadline, it.scheduledDate) }

    /** Whether the draft can be saved at all.
     *
     * A blank title is the case that matters: `title` is `NOT NULL`, so it
     * cannot be cleared, and a save that silently dropped the field would
     * report success while changing nothing — the one outcome worse than a
     * refusal. A malformed date is refused here rather than sent for the
     * authority to 400 into the dead-letter journal. */
    val canSave: Boolean
        get() {
            val draft = _draft.value ?: return false
            if (!hasContentFn(draft.title)) return false
            // #782: a link name beside no URL is the authority's 400,
            // refused here for the same reason a malformed date is.
            if (hasContentFn(draft.linkLabel) && !hasContentFn(draft.linkUrl)) return false
            val problems = metaProblemsFn(draft.deadline, draft.scheduledDate)
            return problems.deadline == null && problems.scheduledDate == null
        }

    /** Whether the draft differs from the seed it started from — what
     * decides whether Back must ask before discarding. An untouched draft
     * is never fought over, and a *sync* is not a human edit: see [_seed]
     * for why this reads the stored seed and never the current record. */
    val isDirty: Boolean
        get() {
            val current = _draft.value ?: return false
            return current != _seed.value
        }

    /** Loads the item: read, and on a miss sync once and read again.
     *
     * **A reload never disturbs a dirty draft.** The sync cadence above
     * the NavHost ticks every 60 seconds; rebuilding the draft from the
     * freshly-read record would erase whatever the human had typed in
     * between, which is the same silent loss the ViewModel exists to
     * prevent. A *clean* draft is reseeded, so a change landing from
     * another device does show through instead of being masked by a stale
     * copy of itself. */
    suspend fun load(itemId: String, nowMs: Long) {
        if (_draft.value == null) _state.value = ItemDetailState.Loading
        try {
            fetchFn(itemId, nowMs)?.let {
                _state.value = ItemDetailState.Loaded(it)
                reseedIfClean(it)
                _statusLine.value = null
                _hasGrillDraft.value = hasGrillDraftFn(itemId)
                return
            }
            syncFn()
            val afterSync = fetchFn(itemId, nowMs)
            _state.value = afterSync
                ?.let { ItemDetailState.Loaded(it) }
                ?: ItemDetailState.NotSynced
            afterSync?.let { reseedIfClean(it) }
            _statusLine.value = null
            _hasGrillDraft.value = afterSync?.let { hasGrillDraftFn(itemId) } ?: false
        } catch (error: Exception) {
            _state.value = ItemDetailState.NotSynced
            _statusLine.value = "Couldn't read this item — ${error.message}"
        }
    }

    /** Seeds the draft on first load, and re-seeds it on any later load
     * that finds nothing to lose. */
    private fun reseedIfClean(record: ItemDetailRecord) {
        if (_draft.value != null && isDirty) return
        val fresh = ItemDraft.of(record)
        _seed.value = fresh
        _draft.value = fresh
    }

    fun updateDraft(draft: ItemDraft) {
        _draft.value = draft
    }

    /** Throws the typed changes away, back to the values the draft was
     * seeded from. Only ever called once the human has said so — see the
     * panel's discard confirmation. The draft does not become null: there
     * is no read mode to fall back to. */
    fun discardDraft() {
        _draft.value = _seed.value
    }

    /** Saves the draft as one CAS patch, then re-reads. The seed advances
     * only on success — a failed save leaves the human's words where they
     * can still be seen and retried, still dirty, still guarded by Back.
     *
     * **Not reachable from Triage.** The panel there submits through
     * [promote] instead (#360 bans a non-promoting write from that
     * surface); this ViewModel is shared, so the enforcement is the
     * panel's mode plus the structural pins named in
     * [ItemDetailPanelMode]'s own doc. */
    suspend fun save(itemId: String, nowMs: Long): Boolean =
        submit(itemId, nowMs, refusal = "This edit can't be saved yet", verb = "save", send = editFn)

    /** Promotes the item to Ready, carrying whatever else the draft
     * touched — one CAS `PATCH` through `Core::triage`, per its own doc.
     * The Triage host's only submit: promotion is the sole destination
     * that surface offers (#360). */
    suspend fun promote(itemId: String, nowMs: Long): Boolean =
        submit(
            itemId,
            nowMs,
            refusal = "This can't be promoted yet",
            verb = "promote",
            send = promoteFn,
        )

    /** What the two submits share: refuse an unsendable draft in the
     * caller's own words, send one patch, and on success re-read and let
     * the reseed advance the seed.
     *
     * **Returns whether the write landed**, because the caller has a
     * decision riding on it: a host that closes its pane on a submit
     * (Triage does — the item leaves its queue) must not close it on a
     * *refused* one, or it unmounts both the refusal message and the draft
     * that caused it. Refusals are worded into [statusLine] rather than
     * thrown, so a `Unit` return left the caller unable to tell. */
    private suspend fun submit(
        itemId: String,
        nowMs: Long,
        refusal: String,
        verb: String,
        send: suspend (String, ItemEdit, Long) -> Unit,
    ): Boolean {
        val record = (_state.value as? ItemDetailState.Loaded)?.record ?: return false
        val draft = _draft.value ?: return false
        // Recall's rule (#478) enforced where it is acted on, not only
        // where it is rendered: the panel opens no editor and draws no
        // submit for a non-editable item, and this is the second lock on
        // the same door — history stays readable.
        if (!record.isEditable) {
            _statusLine.value = "This item is history — readable, not editable."
            return false
        }
        if (!canSave) {
            _statusLine.value = "$refusal — an item needs a title, " +
                "a date must be the shape shown, and a link name needs a URL."
            return false
        }
        try {
            send(itemId, draft.toEdit(record, hasContentFn), nowMs)
            // The sent draft becomes the seed: what was typed is now what
            // the item says, so Back has nothing left to fight over even
            // before the re-read lands.
            _seed.value = draft
        } catch (error: Exception) {
            _statusLine.value = "Couldn't $verb — ${error.message}"
            return false
        }
        load(itemId, nowMs)
        return true
    }

    /** One act from the item pane. Completing or cancelling also acks the
     * live alert about this item — decided in the core, not here.
     *
     * Returns whether the act landed, for [submit]'s reason: a mark-done
     * takes the item out of the Triage queue, so that host closes its pane
     * on one — but only on one that happened. The re-read runs either way,
     * because a failed act still leaves a board worth refreshing. */
    suspend fun act(itemId: String, action: String, nowMs: Long): Boolean {
        val failure = try {
            actFn(itemId, action, nowMs)
            null
        } catch (error: Exception) {
            "Couldn't $action — ${error.message}"
        }
        load(itemId, nowMs)
        failure?.let { _statusLine.value = it }
        return failure == null
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
                // The literal `true` IS #360: a Triage submit promotes, and
                // there is no path through this ViewModel that triages
                // without promoting. Pinned by
                // `ItemDetailPanelStructuralTest`.
                promoteFn = { itemId, edit, nowMs ->
                    CoreHolder.get(context.applicationContext)
                        .triageItem(itemId, true, edit, nowMs)
                },
                hasGrillDraftFn = { itemId ->
                    CoreHolder.get(context.applicationContext).hasGrillDraft(itemId)
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
                hasContentFn = ::canSubmitCapture,
                metaProblemsFn = ::captureMetaProblems,
                formMetaFn = ::captureFormMeta,
            )

        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
