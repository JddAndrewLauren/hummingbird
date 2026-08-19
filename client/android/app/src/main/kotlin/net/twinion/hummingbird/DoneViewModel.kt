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
import uniffi.hummingbird_ffi_mobile.MobileDoneRecord

// The Done screen's read (M3/#532), over the same "ViewModel over
// CoreHolder, injected-fn wiring" shape `AlertsViewModel` established —
// injected so a plain JVM test drives the control flow with no
// host-architecture `.so`.
//
// **This class decides nothing about ordering.** [MobileDoneRecord]s arrive
// from [MobileTaskHost.doneItems] already most-recently-touched first
// (`hummingbird_core::decisions::roster::order_done`) — the module doc's
// Android-never-re-orders rule applies here exactly as it does to
// `NowViewModel`'s board. Read-only by decision: there is no reopen in the
// act vocabulary, and none is invented here (`DoneScreen.tsx`'s own rule,
// ported).
class DoneViewModel(
    private val fetchDoneFn: suspend () -> List<MobileDoneRecord>,
) : ViewModel() {

    private val _items = MutableStateFlow<List<MobileDoneRecord>>(emptyList())
    val items: StateFlow<List<MobileDoneRecord>> = _items.asStateFlow()

    private val _loading = MutableStateFlow(true)
    val loading: StateFlow<Boolean> = _loading.asStateFlow()

    private val _statusLine = MutableStateFlow<String?>(null)
    val statusLine: StateFlow<String?> = _statusLine.asStateFlow()

    suspend fun refresh() {
        _loading.value = true
        try {
            _items.value = fetchDoneFn()
            _statusLine.value = null
        } catch (error: Exception) {
            _statusLine.value = "Couldn't read Done — ${error.message}"
        } finally {
            _loading.value = false
        }
    }

    companion object {
        fun create(context: Context): DoneViewModel =
            DoneViewModel(
                fetchDoneFn = { CoreHolder.get(context.applicationContext).doneItems() },
            )

        /** Activity-scoped, `AlertsViewModel.factory`'s own reasoning: a
         * fold/unfold recreates the Activity on the only install target
         * (a Pixel Fold), and `remember` does not survive that. */
        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
