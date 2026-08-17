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
import uniffi.hummingbird_ffi_mobile.AlertRecord

// The alerts surface's read/ack pair (M2/#141, ADR-0012), over the same
// "ViewModel over CoreHolder, injected-fn wiring" shape `NowViewModel`
// (M1-6/#504) established — injected so a plain JVM test drives the control
// flow with no host-architecture `.so`.
//
// The same asymmetry `NowViewModel` documents applies here, and ADR-0014
// makes it sharper: an [AlertRecord] arrives already decided. `isLive` is
// the three-clause liveness predicate applied against the caller's clock,
// and `canAck` is whether the gesture is worth offering. A Kotlin
// `dismissedAt == null` test is the exact bug that predicate exists to
// prevent — it cannot tell an expired-then-re-raised occurrence from an
// acked one, and `expires_at` is never written back as a dismissal. So
// neither this class nor `AlertsScreen` reads `dismissedAt` at all, and a
// structural test enforces that.
//
// Ordering is core's too (`Core::live_alerts` sorts by `raised_at`
// descending, id as tiebreak). This list is rendered in the order it
// arrives; there is no comparator on this side of the seam.
class AlertsViewModel(
    private val fetchAlertsFn: suspend (nowMs: Long) -> List<AlertRecord>,
    private val ackFn: suspend (alertId: String, nowMs: Long) -> Unit,
) : ViewModel() {

    private val _alerts = MutableStateFlow<List<AlertRecord>>(emptyList())
    val alerts: StateFlow<List<AlertRecord>> = _alerts.asStateFlow()

    private val _loading = MutableStateFlow(true)
    val loading: StateFlow<Boolean> = _loading.asStateFlow()

    /** The honest line when something failed, or null. Not an error
     * dialog: an alert lane that cannot ack right now is a fact to report
     * where the user is already looking, not a modal to dismiss. */
    private val _statusLine = MutableStateFlow<String?>(null)
    val statusLine: StateFlow<String?> = _statusLine.asStateFlow()

    /** Reloads the live alerts. `nowMs` is wall-clock milliseconds — alert
     * liveness is instants throughout, with no civil date to resolve (see
     * `MobileTaskHost.alerts`). */
    suspend fun refresh(nowMs: Long) {
        _loading.value = true
        try {
            _alerts.value = fetchAlertsFn(nowMs)
            _statusLine.value = null
        } catch (error: Exception) {
            _statusLine.value = "Couldn't read alerts — ${error.message}"
        } finally {
            _loading.value = false
        }
    }

    /** Acks, then reloads so the row's own `canAck` (and its departure from
     * the live list) reflects the mutation immediately — local-first, the
     * same criterion `NowViewModel.act` leans on.
     *
     * A failure leaves the list alone and says so. The row is still there
     * and still `canAck`, so the gesture stays available; that is the
     * durable retry, not a spinner. */
    suspend fun ack(alertId: String, nowMs: Long) {
        val failure = try {
            ackFn(alertId, nowMs)
            null
        } catch (error: Exception) {
            "Couldn't ack — ${error.message}"
        }
        // Reload first, then report: [refresh] clears the line on success,
        // so setting it before the reload would wipe the very thing the
        // user needs to see.
        refresh(nowMs)
        failure?.let { _statusLine.value = it }
    }

    companion object {
        /** The production wiring: both fns close over the app's one
         * durable [CoreHolder] handle — never a fresh core per call. */
        fun create(context: Context): AlertsViewModel =
            AlertsViewModel(
                fetchAlertsFn = { nowMs ->
                    CoreHolder.get(context.applicationContext).alerts(nowMs)
                },
                ackFn = { alertId, nowMs ->
                    CoreHolder.get(context.applicationContext).ackAlert(alertId, nowMs)
                },
            )

        /** The factory `AlertsScreen` hands to `viewModel()`, so the loaded
         * list is scoped to the Activity's `ViewModelStore` rather than to
         * a composition — `remember` does not survive Activity recreation,
         * and on the only install target (a Pixel Fold) a fold/unfold
         * recreates. Same correction `CaptureViewModel.factory` documents. */
        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
