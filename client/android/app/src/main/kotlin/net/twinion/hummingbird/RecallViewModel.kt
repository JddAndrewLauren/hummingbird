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
import uniffi.hummingbird_ffi_mobile.MobileRecallOutcome
import uniffi.hummingbird_ffi_mobile.MobileRecallRowRecord

// Recall (#542/#478): the phone's re-find gesture over everything the
// mirror has ever known, the same "ViewModel over CoreHolder, injected-fn
// wiring" shape `LedgerViewModel` established.
//
// **This class decides nothing about a row or the result set.**
// `MobileTaskHost.search` (`hummingbird_core::search`, ADR-0025) hands back
// rows already matched, grouped (live/done/archived) and ordered — most
// recently touched first within a group, capped, with the un-capped `total`
// alongside. There is no sort, filter or group-by anywhere in this file;
// `RecallScreenStructuralTest` gates that the same way
// `RulesScreenStructuralTest` gates its own surface.
//
// **`query` and `search` are deliberately two different members.**
// `setQueryText` is a plain (non-suspend) setter `RecallOverlay`'s text
// field calls on every keystroke; `search` is what actually reaches the
// seam, invoked from a `LaunchedEffect(query)` keyed on the field it just
// set — the same split `TriageScreen`'s draft field keeps between "what's
// typed" and "what's asked", and the only way a fast typist's field never
// stalls behind an in-flight crossing.
//
// **No debounce.** `Core::search` is a read-time scan over the mirror
// (ADR-0002), not a network request — there is no round trip to spare a
// request against, so every keystroke simply asks again, the same rule
// `useRecallWiring.ts` states for the web.
//
// **The blank-query short-circuit.** `Core::search` itself already answers
// an empty or whitespace-only query with no rows and a zero total (that
// module's own doc: "recall is never browse everything") — trimming and
// checking here duplicates that one *fact*, never its matching, grouping or
// ordering, to skip a pointless crossing and to give `RecallOverlay` a
// distinct "type to search" state before any answer would exist. The web's
// `useRecallWiring.ts` keeps the identical duplicate for the identical
// reason.
//
// **`rows` is not cleared when `query` changes — but the SELECTION is.**
// The web's `useRecallWiring.ts` clears its whole slot on every keystroke
// because a stale row there carries its own editable field state. Here a
// tapped row expands into `ItemDetailPanel` (the search-overlay slice),
// which re-reads the item by id — so a lagging collapsed row is a display
// lag, not a correctness risk, and stays to avoid flashing an empty list
// on every keystroke. The expanded panel, though, must not stand open
// under a query it no longer answers: `setQueryText` closes it.
class RecallViewModel(
    private val searchFn: suspend (query: String, nowMs: Long) -> MobileRecallOutcome,
) : ViewModel() {

    private val _query = MutableStateFlow("")
    val query: StateFlow<String> = _query.asStateFlow()

    private val _rows = MutableStateFlow<List<MobileRecallRowRecord>>(emptyList())
    val rows: StateFlow<List<MobileRecallRowRecord>> = _rows.asStateFlow()

    private val _total = MutableStateFlow(0u)
    val total: StateFlow<UInt> = _total.asStateFlow()

    private val _loading = MutableStateFlow(false)
    val loading: StateFlow<Boolean> = _loading.asStateFlow()

    private val _statusLine = MutableStateFlow<String?>(null)
    val statusLine: StateFlow<String?> = _statusLine.asStateFlow()

    /** The one expanded row (the search-overlay slice) — here, never in a
     * `remember {}`, for the recorded fold/unfold reason every selection
     * on this app holds: a composition-scoped selection is lost on
     * Activity recreation. */
    private val _selectedId = MutableStateFlow<String?>(null)
    val selectedId: StateFlow<String?> = _selectedId.asStateFlow()

    fun setQueryText(text: String) {
        _query.value = text
        _selectedId.value = null
    }

    /** Toggles a row's inline panel — tapping the open row again closes
     * it, `NowViewModel.selectItem`'s own contract. */
    fun select(itemId: String) {
        _selectedId.value = if (_selectedId.value == itemId) null else itemId
    }

    fun clearSelection() {
        _selectedId.value = null
    }

    suspend fun search(nowMs: Long) {
        val trimmed = _query.value.trim()
        if (trimmed.isEmpty()) {
            _rows.value = emptyList()
            _total.value = 0u
            _statusLine.value = null
            return
        }
        _loading.value = true
        try {
            val outcome = searchFn(_query.value, nowMs)
            _rows.value = outcome.rows
            _total.value = outcome.total
            _statusLine.value = null
        } catch (error: CancellationException) {
            // `LaunchedEffect(query)` cancels this coroutine on every
            // keystroke that lands mid-crossing — the common case for a
            // fast typist, and unlike `LedgerViewModel`/`RulesViewModel`
            // (cancelled only by a screen leave), Recall cancels on nearly
            // every call. Swallowing it into `statusLine` the way the
            // catch below does would flash "Couldn't search" on ordinary
            // typing; the next keystroke's own search is the real answer,
            // so this one must propagate rather than be reported.
            throw error
        } catch (error: Exception) {
            _statusLine.value = "Couldn't search — ${error.message}"
        } finally {
            _loading.value = false
        }
    }

    companion object {
        fun create(context: Context): RecallViewModel =
            RecallViewModel(
                searchFn = { query, nowMs -> CoreHolder.get(context.applicationContext).search(query, nowMs) },
            )

        /** Activity-scoped, `LedgerViewModel.factory`'s own reasoning: a
         * fold/unfold recreates the Activity on the only install target
         * (a Pixel Fold), and `remember` does not survive that. */
        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
