package net.twinion.hummingbird

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.twinion.hummingbird.core.CoreHolder
import uniffi.hummingbird_ffi_mobile.CaptureFormMeta
import uniffi.hummingbird_ffi_mobile.FieldPatch
import uniffi.hummingbird_ffi_mobile.ItemEdit
import uniffi.hummingbird_ffi_mobile.MetaProblems
import uniffi.hummingbird_ffi_mobile.TriageBoardRecord
import uniffi.hummingbird_ffi_mobile.TriageItemRecord
import uniffi.hummingbird_ffi_mobile.canSubmitCapture
import uniffi.hummingbird_ffi_mobile.captureFormMeta
import uniffi.hummingbird_ffi_mobile.captureMetaProblems

/** What the Triage screen is showing — a single state, unlike
 * [ItemDetailState]'s three: there is no deep-link race here (nothing
 * lands on this screen from a notification), so "no board yet" only ever
 * means "still loading". */
sealed interface TriageState {
    data object Loading : TriageState

    data class Loaded(val board: TriageBoardRecord) : TriageState
}

/** One row's in-progress edit — every editable field the seeded editor
 * shows, seeded from the [TriageItemRecord] it opened on. The same
 * strings-throughout shape [ItemDraft] uses, and [toEdit] makes the
 * identical three-way Untouched/Clear/Set decision that class's own doc
 * explains; the two are not merged into one type because a triage row and
 * item detail seed from different records ([TriageItemRecord] vs.
 * [uniffi.hummingbird_ffi_mobile.ItemDetailRecord]) and edit different
 * seam calls.
 *
 * `projectId` is absent for the same reason [ItemDraft]'s is: not editable
 * from this screen, and a field with nothing to set it from would be a
 * silent no-op.
 */
data class TriageDraft(
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
        fun of(item: TriageItemRecord) = TriageDraft(
            title = item.title,
            description = item.description.orEmpty(),
            context = item.context.orEmpty(),
            deadline = item.deadline.orEmpty(),
            scheduledDate = item.scheduledDate.orEmpty(),
            size = item.size.orEmpty(),
            energy = item.energy.orEmpty(),
            priority = item.priority.toString(),
        )
    }

    /** The draft as a seam patch, against the row it started from. See
     * [ItemDraft.toEdit]'s own doc for the full reasoning — this is the
     * same rule, seeded from a [TriageItemRecord] instead of an
     * [uniffi.hummingbird_ffi_mobile.ItemDetailRecord]. */
    fun toEdit(from: TriageItemRecord, hasContent: (String) -> Boolean): ItemEdit = ItemEdit(
        title = title.takeIf { hasContent(it) && it != from.title },
        priority = priority.toLongOrNull()?.takeIf { it != from.priority },
        description = patch(description, from.description, hasContent),
        size = patch(size, from.size, hasContent),
        energy = patch(energy, from.energy, hasContent),
        context = patch(context, from.context, hasContent),
        projectId = FieldPatch.Untouched,
        deadline = patch(deadline, from.deadline, hasContent),
        scheduledDate = patch(scheduledDate, from.scheduledDate, hasContent),
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

// The Triage screen's whole read and its one-row-open editor (#531): one
// queue holding both captured and Grilling items in the core's own order
// (`MobileTaskHost::triageBoard`), promote-to-Ready as the only mutation
// destination this screen offers, and the checkmark's mark-done gesture
// riding on `Core::act` — never a triage.
//
// **The selected row and its draft live here, never in a `remember {}`.**
// `ItemDetailViewModel`'s own recorded defect (the fold/unfold one)
// applies verbatim: a composition-scoped draft is lost on Activity
// recreation, and a draft is human-authored content.
class TriageViewModel(
    private val fetchFn: suspend () -> TriageBoardRecord,
    private val triageFn: suspend (itemId: String, promoteToReady: Boolean, edit: ItemEdit, nowMs: Long) -> Unit,
    private val completeFn: suspend (itemId: String, nowMs: Long) -> Unit,
    /** The core's blank rule, injected the same way [ItemDetailViewModel]'s
     * is: no native library exists in a plain JVM test process. */
    private val hasContentFn: (String) -> Boolean,
    /** The core's date-field rule, injected for the same reason. */
    private val metaProblemsFn: (deadline: String, scheduledDate: String) -> MetaProblems,
    /** The shared form components' vocabulary door, injected the same way
     * `CaptureViewModel.formMetaFn` is: every size/energy word this
     * screen's editor offers must come from here, never a Kotlin literal. */
    private val formMetaFn: () -> CaptureFormMeta,
) : ViewModel() {

    /** Read once, on first use — the vocabulary does not change mid-session,
     * the same laziness `CaptureViewModel.formMeta` uses. */
    val formMeta: CaptureFormMeta by lazy { formMetaFn() }

    private val _state = MutableStateFlow<TriageState>(TriageState.Loading)
    val state: StateFlow<TriageState> = _state.asStateFlow()

    private val _statusLine = MutableStateFlow<String?>(null)
    val statusLine: StateFlow<String?> = _statusLine.asStateFlow()

    /** The one open row, or null — an inbox is for reading first, same
     * resting state `TriageScreen.tsx`'s own `selectedId` starts in. */
    private val _selectedId = MutableStateFlow<String?>(null)
    val selectedId: StateFlow<String?> = _selectedId.asStateFlow()

    private val _draft = MutableStateFlow<TriageDraft?>(null)
    val draft: StateFlow<TriageDraft?> = _draft.asStateFlow()

    val metaProblems: MetaProblems?
        get() = _draft.value?.let { metaProblemsFn(it.deadline, it.scheduledDate) }

    /** Whether the open row's draft can be saved (promoted or edited) at
     * all — [ItemDetailViewModel.canSave]'s own reasoning, applied to
     * whichever row is open. */
    val canSave: Boolean
        get() {
            val draft = _draft.value ?: return false
            if (!hasContentFn(draft.title)) return false
            val problems = metaProblemsFn(draft.deadline, draft.scheduledDate)
            return problems.deadline == null && problems.scheduledDate == null
        }

    private fun currentItems(): List<TriageItemRecord> =
        (_state.value as? TriageState.Loaded)?.board?.items.orEmpty()

    /** Loads the whole queue: a plain read, on the app-wide cadence hoisted
     * above the `NavHost` (`syncTick`, `MainActivity`'s own doc) — the same
     * "no screen-local sync" shape `NowScreen` uses, since a Triage row is
     * exactly as live as a frontier row and the two already share one
     * cycle. A reload never disturbs an open draft — the same "the
     * 60-second cadence must not erase what the human is typing" rule
     * [ItemDetailViewModel.load]'s own doc states. */
    suspend fun load() {
        try {
            _state.value = TriageState.Loaded(fetchFn())
            _statusLine.value = null
        } catch (error: Exception) {
            _statusLine.value = "Couldn't read Triage — ${error.message}"
        }
    }

    /** Toggles a row open or closed. Only ever one row open at a time —
     * opening a different row replaces whatever was open, discarding its
     * unsent draft, the same "selection, not accumulation" contract the
     * web `TriageScreen`'s own `selectedId` state carries. */
    fun select(itemId: String) {
        if (_selectedId.value == itemId) {
            _selectedId.value = null
            _draft.value = null
            return
        }
        val item = currentItems().find { it.id == itemId } ?: return
        _selectedId.value = itemId
        _draft.value = TriageDraft.of(item)
        _statusLine.value = null
    }

    fun updateDraft(draft: TriageDraft) {
        _draft.value = draft
    }

    /** Promotes the open row to Ready, carrying whatever else the draft
     * touched — one CAS `PATCH`, per `Core::triage`'s own doc. Promotion is
     * the only destination this screen offers (the AC this method exists
     * to satisfy): there is no "save without promoting" affordance here,
     * unlike item detail's own edit mode. */
    suspend fun promote(itemId: String, nowMs: Long) {
        val item = currentItems().find { it.id == itemId } ?: return
        val draft = _draft.value ?: return
        if (!canSave) {
            _statusLine.value = "This can't be promoted yet — an item needs a title, " +
                "and a date must be the shape shown."
            return
        }
        try {
            triageFn(itemId, true, draft.toEdit(item, hasContentFn), nowMs)
            _selectedId.value = null
            _draft.value = null
        } catch (error: Exception) {
            _statusLine.value = "Couldn't promote — ${error.message}"
            return
        }
        load()
    }

    /** The row checkmark: `Core::act`'s `complete`, never a triage — a
     * capture that turned out already finished skips the editor entirely,
     * the same amendment `TriageRow.tsx`'s own doc records. */
    suspend fun complete(itemId: String, nowMs: Long) {
        val failure = try {
            completeFn(itemId, nowMs)
            null
        } catch (error: Exception) {
            "Couldn't complete — ${error.message}"
        }
        if (_selectedId.value == itemId) {
            _selectedId.value = null
            _draft.value = null
        }
        load()
        failure?.let { _statusLine.value = it }
    }

    companion object {
        fun create(context: Context): TriageViewModel =
            TriageViewModel(
                fetchFn = { CoreHolder.get(context.applicationContext).triageBoard() },
                triageFn = { itemId, promoteToReady, edit, nowMs ->
                    CoreHolder.get(context.applicationContext)
                        .triageItem(itemId, promoteToReady, edit, nowMs)
                },
                completeFn = { itemId, nowMs ->
                    CoreHolder.get(context.applicationContext).act(itemId, "complete", nowMs)
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
