package net.twinion.hummingbird

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.twinion.hummingbird.core.CoreHolder
import uniffi.hummingbird_ffi_mobile.TriageBoardRecord
import uniffi.hummingbird_ffi_mobile.TriageItemRecord

/** What the Triage screen is showing — a single state, unlike
 * [ItemDetailState]'s three: there is no deep-link race here (nothing
 * lands on this screen from a notification), so "no board yet" only ever
 * means "still loading". */
sealed interface TriageState {
    data object Loading : TriageState

    data class Loaded(val board: TriageBoardRecord) : TriageState
}

// The Triage screen's board and its selection (#531): one queue holding
// both captured and Grilling items in the core's own order
// (`MobileTaskHost::triageBoard`), and the checkmark's mark-done gesture
// riding on `Core::act` — never a triage.
//
// **The draft is not here.** It used to be: this ViewModel owned a
// `TriageDraft` and a `promote` beside it, duplicating
// `ItemDetailViewModel`'s. The opened row now renders `ItemDetailPanel` in
// `ItemDetailPanelMode.PROMOTE`, which brings that ViewModel with it — one
// draft type, one patch rule, one promote. What stays here is what a board
// owns: the rows, the status line, and which one is open.
//
// **The selected row lives here, never in a `remember {}`.**
// `ItemDetailViewModel`'s own recorded defect (the fold/unfold one)
// applies: composition-scoped state is lost on Activity recreation.
class TriageViewModel(
    private val fetchFn: suspend (now: String) -> TriageBoardRecord,
    private val completeFn: suspend (itemId: String, nowMs: Long) -> Unit,
) : ViewModel() {

    private val _state = MutableStateFlow<TriageState>(TriageState.Loading)
    val state: StateFlow<TriageState> = _state.asStateFlow()

    private val _statusLine = MutableStateFlow<String?>(null)
    val statusLine: StateFlow<String?> = _statusLine.asStateFlow()

    /** The one open row, or null — an inbox is for reading first, same
     * resting state `TriageScreen.tsx`'s own `selectedId` starts in. */
    private val _selectedId = MutableStateFlow<String?>(null)
    val selectedId: StateFlow<String?> = _selectedId.asStateFlow()

    /** Shuts whichever row is open — what a promote or a mark-done from
     * the opened pane needs: the item leaves the queue, so a selection
     * still pointing at it would dangle at a vanished row. */
    fun closeSelection() {
        _selectedId.value = null
    }

    private fun currentItems(): List<TriageItemRecord> =
        (_state.value as? TriageState.Loaded)?.board?.items.orEmpty()

    /** Loads the whole queue: a plain read, on the app-wide cadence hoisted
     * above the `NavHost` (`syncTick`, `MainActivity`'s own doc) — the same
     * "no screen-local sync" shape `NowScreen` uses, since a Triage row is
     * exactly as live as a frontier row and the two already share one
     * cycle. The opened pane's own draft is untouched by this: it lives in
     * [ItemDetailViewModel], which reloads on its own terms and leaves a
     * dirty draft alone (that class's [ItemDetailViewModel.load] doc). */
    suspend fun load(now: String) {
        try {
            _state.value = TriageState.Loaded(fetchFn(now))
            _statusLine.value = null
        } catch (error: Exception) {
            _statusLine.value = "Couldn't read Triage — ${error.message}"
        }
    }

    /** Toggles a row open or closed. Only ever one row open at a time —
     * opening a different row replaces whatever was open, the same
     * "selection, not accumulation" contract the web `TriageScreen`'s own
     * `selectedId` state carries. What the closed row had typed is not
     * lost: its draft belongs to that item's own [ItemDetailViewModel],
     * keyed `"item-$id"` and held by this screen's back-stack entry, so
     * re-opening the row shows it again. */
    fun select(itemId: String) {
        if (_selectedId.value == itemId) {
            _selectedId.value = null
            return
        }
        // Membership still gates it: a selection is only ever a row on the
        // board this ViewModel is holding.
        currentItems().find { it.id == itemId } ?: return
        _selectedId.value = itemId
        _statusLine.value = null
    }

    /** The row checkmark: `Core::act`'s `complete`, never a triage — a
     * capture that turned out already finished skips the editor entirely,
     * the same amendment `TriageRow.tsx`'s own doc records. The open row
     * closes on success — and only on success: a failed act leaves the row
     * on the board, so its pane stays where [statusLine] can be read
     * against it and the act retried. Cancellation rethrows rather than
     * being worded as a failure. */
    suspend fun complete(itemId: String, now: String, nowMs: Long) {
        val failure = try {
            completeFn(itemId, nowMs)
            null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            "Couldn't complete — ${error.message}"
        }
        if (failure == null && _selectedId.value == itemId) {
            _selectedId.value = null
        }
        load(now)
        failure?.let { _statusLine.value = it }
    }

    companion object {
        fun create(context: Context): TriageViewModel =
            TriageViewModel(
                fetchFn = { now -> CoreHolder.get(context.applicationContext).triageBoard(now) },
                completeFn = { itemId, nowMs ->
                    CoreHolder.get(context.applicationContext).act(itemId, "complete", nowMs)
                },
            )

        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
