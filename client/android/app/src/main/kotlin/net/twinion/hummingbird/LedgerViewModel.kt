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
import uniffi.hummingbird_ffi_mobile.MobileLedgerRowRecord

// The Ledger's read/mark-done pair (M3/#532), the same
// "ViewModel over CoreHolder, injected-fn wiring" shape `AlertsViewModel`
// established.
//
// **This class decides nothing about a row.** [MobileLedgerRowRecord]s
// arrive from [MobileTaskHost.ledgerRows] pre-ordered
// (`hummingbird_core::decisions::roster::order_ledger`, last touched
// first) and pre-gated (`canMarkDone` mirrors `item-actions.ts`'s widened
// one-click rule) — there is no `archivedAt == null` re-derivation here,
// the same discipline `AlertsViewModel`'s own doc states for
// `dismissedAt`. `LedgerScreenStructuralTest` (if this screen ever grows
// one, the same pattern `RulesScreenStructuralTest` uses) would refuse a
// re-derivation the same way.
class LedgerViewModel(
    private val fetchRowsFn: suspend (nowMs: Long) -> List<MobileLedgerRowRecord>,
    private val completeFn: suspend (itemId: String, nowMs: Long) -> Unit,
) : ViewModel() {

    private val _rows = MutableStateFlow<List<MobileLedgerRowRecord>>(emptyList())
    val rows: StateFlow<List<MobileLedgerRowRecord>> = _rows.asStateFlow()

    private val _loading = MutableStateFlow(true)
    val loading: StateFlow<Boolean> = _loading.asStateFlow()

    private val _statusLine = MutableStateFlow<String?>(null)
    val statusLine: StateFlow<String?> = _statusLine.asStateFlow()

    suspend fun refresh(nowMs: Long) {
        _loading.value = true
        try {
            _rows.value = fetchRowsFn(nowMs)
            _statusLine.value = null
        } catch (error: Exception) {
            _statusLine.value = "Couldn't read the Ledger — ${error.message}"
        } finally {
            _loading.value = false
        }
    }

    /** The Ledger's one-click mark-done. Completes, then reloads so the
     * row's own state (and its departure from the live set the checkmark
     * gates on) reflects the mutation immediately — `AlertsViewModel.ack`'s
     * own shape. A failure leaves the row alone and says so; the checkmark
     * is still there and still offered, which is the durable retry. */
    suspend fun complete(itemId: String, nowMs: Long) {
        val failure = try {
            completeFn(itemId, nowMs)
            null
        } catch (error: Exception) {
            "Couldn't mark that done — ${error.message}"
        }
        refresh(nowMs)
        failure?.let { _statusLine.value = it }
    }

    companion object {
        fun create(context: Context): LedgerViewModel =
            LedgerViewModel(
                fetchRowsFn = { nowMs -> CoreHolder.get(context.applicationContext).ledgerRows(nowMs) },
                completeFn = { itemId, nowMs ->
                    CoreHolder.get(context.applicationContext).act(itemId, "complete", nowMs)
                },
            )

        /** Activity-scoped, `AlertsViewModel.factory`'s own reasoning: a
         * fold/unfold recreates the Activity on the only install target
         * (a Pixel Fold), and `remember` does not survive that. */
        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
