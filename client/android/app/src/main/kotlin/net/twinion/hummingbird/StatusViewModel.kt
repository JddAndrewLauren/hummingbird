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
import net.twinion.hummingbird.core.SyncHistoryStore
import net.twinion.hummingbird.ui.panes.CollapseOverride
import net.twinion.hummingbird.ui.panes.PaneCollapse
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileSurface

/** What the Status screen is showing — [Loading] until the first rank has
 * landed, which is real and reachable (the seam crossing is a JNI call
 * plus a `context_snapshots` read, never instant on a slow device), never
 * an empty list masquerading as "nothing to report" — ADR-0015's rule,
 * carried across to this client. */
sealed interface StatusState {
    data object Loading : StatusState

    /** [rankedAtMs] is the clock the rank was taken at — what the shell's
     * age/countdown words render against. */
    data class Loaded(val panes: List<MobileRankedPane>, val rankedAtMs: Long) : StatusState
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
    /** The pane collapse's device-local store (`PanePrefs`, surface-keyed
     * to [MobileSurface.STATUS]) — injected like [rankPanesFn] so a JVM
     * test needs no DataStore. */
    private val readPaneCollapseFn: suspend () -> Map<String, CollapseOverride> = { emptyMap() },
    private val writePaneCollapseFn: suspend (Map<String, CollapseOverride>) -> Unit = {},
) : ViewModel() {

    private val _state = MutableStateFlow<StatusState>(StatusState.Loading)
    val state: StateFlow<StatusState> = _state.asStateFlow()

    /** The band-stamped collapse overrides (`PaneCollapse`) — loaded once
     * with the first rank, written through [togglePaneCollapsed]. Here,
     * never in a `remember {}`: the recorded fold/unfold defect. */
    private val _paneOverrides = MutableStateFlow<Map<String, CollapseOverride>>(emptyMap())
    val paneOverrides: StateFlow<Map<String, CollapseOverride>> = _paneOverrides.asStateFlow()
    private var paneOverridesLoaded = false

    /** The one failure line this screen owns — `TriageViewModel`'s
     * `statusLine` shape, cleared by the next load that lands. */
    private val _statusLine = MutableStateFlow<String?>(null)
    val statusLine: StateFlow<String?> = _statusLine.asStateFlow()

    suspend fun load(nowMs: Long) {
        try {
            _state.value = StatusState.Loaded(rankPanesFn(nowMs), nowMs)
            _statusLine.value = null
            if (!paneOverridesLoaded) {
                _paneOverrides.value = readPaneCollapseFn()
                paneOverridesLoaded = true
            }
        } catch (error: CancellationException) {
            // A resume cancelled by a fold or a fast Back is not a failure
            // to report (`NowViewModel.refresh`'s own rule).
            throw error
        } catch (error: Exception) {
            // The rank is a JNI crossing that can throw `InternalException`;
            // unhandled inside a resume effect it takes the Activity down.
            // Worded instead — and [state] stays as it was, so a screen that
            // had panes keeps showing them under the line.
            _statusLine.value = "Couldn't read Status — ${error.message}"
        }
    }

    /** Flips one pane's collapse, stamped with the band it was made in and
     * pruned of unranked keys — `PaneCollapse.write`'s whole contract. */
    suspend fun togglePaneCollapsed(pane: MobileRankedPane) {
        val current = _paneOverrides.value
        val resolved = PaneCollapse.resolve(current, pane.paneKey, pane.answer)
        val ranked = (state.value as? StatusState.Loaded)?.panes.orEmpty().map { it.paneKey }
        val next = PaneCollapse.write(
            current,
            pane.paneKey,
            CollapseOverride(pane.answer.band, !resolved),
            ranked,
        )
        _paneOverrides.value = next
        writePaneCollapseFn(next)
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
                readPaneCollapseFn = { PanePrefs.readCollapse(appContext, MobileSurface.STATUS) },
                writePaneCollapseFn = { map ->
                    PanePrefs.writeCollapse(appContext, MobileSurface.STATUS, map)
                },
            )
        }

        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
