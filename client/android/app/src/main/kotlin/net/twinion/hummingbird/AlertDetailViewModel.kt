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
import uniffi.hummingbird_ffi_mobile.AlertRecord

/** What the detail screen is showing. Three states, not two: "this device
 * has not synced that alert yet" is a real, reachable, temporary condition
 * — a notification tap can beat the sync it enqueued — and it is neither a
 * loaded record nor an error. */
sealed interface AlertDetailState {
    data object Loading : AlertDetailState

    data class Loaded(val record: AlertRecord) : AlertDetailState

    /** Synced and still absent. The screen offers a manual retry; it never
     * crashes and never invents a placeholder record. */
    data object NotSynced : AlertDetailState
}

// The alert-detail read (M2/#141), injected-fn shaped like every other
// ViewModel here.
//
// Two seam facts shape it, both deliberate on the Rust side:
//
// - `MobileTaskHost.alert` is **not** liveness-filtered. `null` means only
//   "this device has not synced that alert", which is exactly the deep-link
//   race: the push arrives, the user taps immediately, and the cycle the
//   push enqueued has not landed. The answer is to run a cycle and ask
//   once more — never to guess, and never to treat absence as an error the
//   first time.
// - Because it is not liveness-filtered, the screen **survives its own
//   ack**. The record keeps rendering after the gesture, now with
//   `canAck` false, rather than vanishing under the user's finger.
class AlertDetailViewModel(
    private val fetchAlertFn: suspend (alertId: String, nowMs: Long) -> AlertRecord?,
    private val ackFn: suspend (alertId: String, nowMs: Long) -> Unit,
    private val syncFn: suspend () -> Unit,
) : ViewModel() {

    private val _state = MutableStateFlow<AlertDetailState>(AlertDetailState.Loading)
    val state: StateFlow<AlertDetailState> = _state.asStateFlow()

    private val _statusLine = MutableStateFlow<String?>(null)
    val statusLine: StateFlow<String?> = _statusLine.asStateFlow()

    /** Loads the alert: read, and on a miss sync once and read again. */
    suspend fun load(alertId: String, nowMs: Long) {
        _state.value = AlertDetailState.Loading
        try {
            fetchAlertFn(alertId, nowMs)?.let {
                _state.value = AlertDetailState.Loaded(it)
                _statusLine.value = null
                return
            }
            syncFn()
            val afterSync = fetchAlertFn(alertId, nowMs)
            _state.value = afterSync
                ?.let { AlertDetailState.Loaded(it) }
                ?: AlertDetailState.NotSynced
            _statusLine.value = null
        } catch (error: Exception) {
            _state.value = AlertDetailState.NotSynced
            _statusLine.value = "Couldn't read this alert — ${error.message}"
        }
    }

    /** Acks, then re-reads so `canAck` and the acked treatment update in
     * place. The row does not disappear: `alert()` returns settled records
     * too, which is what makes acking from here survivable. */
    suspend fun ack(alertId: String, nowMs: Long) {
        val failure = try {
            ackFn(alertId, nowMs)
            null
        } catch (error: Exception) {
            "Couldn't ack — ${error.message}"
        }
        // Reload first, then report: [load] clears the line on success, so
        // setting it before the reload would wipe the very thing the user
        // needs to see.
        load(alertId, nowMs)
        failure?.let { _statusLine.value = it }
    }

    companion object {
        fun create(context: Context): AlertDetailViewModel =
            AlertDetailViewModel(
                fetchAlertFn = { alertId, nowMs ->
                    CoreHolder.get(context.applicationContext).alert(alertId, nowMs)
                },
                ackFn = { alertId, nowMs ->
                    CoreHolder.get(context.applicationContext).ackAlert(alertId, nowMs)
                },
                // `"push"`, not `"timer"`: the seam maps any non-`"timer"`
                // trigger to `Trigger::User`, which is not backoff-gated.
                // A deep link that lands during a backoff window must
                // still be able to fetch its row.
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
