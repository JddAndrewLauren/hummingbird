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
import net.twinion.hummingbird.core.SyncHistoryStore
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileSurface

/** What the Status screen is showing — [Loading] until the first rank has
 * landed, which is real and reachable (the seam crossing is a JNI call
 * plus a `context_snapshots` read, never instant on a slow device), never
 * an empty list masquerading as "nothing to report" — ADR-0015's rule,
 * carried across to this client. */
sealed interface StatusState {
    data object Loading : StatusState

    data class Loaded(val panes: List<MobileRankedPane>) : StatusState
}

// The Status screen's decision half (#536/M4, ADR-0025): calls
// `MobileTaskHost.rankPanes` and hands back **applied results** —
// `standingQuestion`/`band`/`answerState`, already decided. This class
// makes no pane judgement of its own: it does not parse a payload, does
// not band anything, and does not decide which subjects exist. The zone
// bridge's first phase is not called here at all — [zone_queries] answers
// empty for [MobileSurface.STATUS] today (none of the status four is
// civil-date reasoning), so this screen crosses an empty zone-facts list
// straight through; #537's Now screen is what exercises the resolve leg.
class StatusViewModel(
    private val rankPanesFn: suspend (nowMs: Long) -> List<MobileRankedPane>,
) : ViewModel() {

    private val _state = MutableStateFlow<StatusState>(StatusState.Loading)
    val state: StateFlow<StatusState> = _state.asStateFlow()

    suspend fun load(nowMs: Long) {
        _state.value = StatusState.Loaded(rankPanesFn(nowMs))
    }

    companion object {
        fun create(context: Context): StatusViewModel {
            val appContext = context.applicationContext
            suspend fun core() = CoreHolder.get(appContext)
            return StatusViewModel(
                rankPanesFn = { nowMs ->
                    core().rankPanes(
                        MobileSurface.STATUS,
                        nowMs,
                        emptyList(),
                        SyncHistoryStore.load(appContext),
                    )
                },
            )
        }

        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
