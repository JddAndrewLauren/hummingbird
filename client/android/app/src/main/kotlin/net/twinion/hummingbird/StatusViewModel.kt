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
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileSurface
import uniffi.hummingbird_ffi_mobile.MobileSyncFacts

/** What the Status screen is showing — [Loading] until the first rank has
 * landed, which is real and reachable (the seam crossing is a JNI call
 * plus a `context_snapshots` read, never instant on a slow device), never
 * an empty list masquerading as "nothing to report" — ADR-0015's rule,
 * carried across to this client. */
sealed interface StatusState {
    data object Loading : StatusState

    /** [rankedAtMs] is the clock the rank was taken at — what the shell's
     * age/countdown words render against. [queueDepth] and [apiVersion] are
     * separate seam crossings taken beside the rank — not one atomic read,
     * and not claimed to be: they are read together so the strip and the
     * footer describe the same moment the panes do, within a millisecond,
     * which is all this screen needs. */
    data class Loaded(
        val panes: List<MobileRankedPane>,
        val rankedAtMs: Long,
        val queueDepth: UInt?,
        val apiVersion: UInt?,
        /** This device's own durable sync history — what the sync strip
         * falls back to on a cold start, so it cannot contradict the
         * reachability pane beside it. */
        val syncFacts: MobileSyncFacts?,
    ) : StatusState
}

// The Status screen's decision half (#536/M4, ADR-0025): calls
// `MobileTaskHost.rankPanes` and hands back **applied results** —
// `standingQuestion`/`band`/`answerState`, already decided. This class
// makes no pane judgement of its own: it does not parse a payload, does
// not band anything, and does not decide which subjects exist. The zone
// bridge's first phase is not called here at all — [zone_queries] answers
// empty for [MobileSurface.STATUS] today (none of the status five —
// kimi/github/uptime/reachability/poller — is civil-date reasoning), so
// this screen crosses an empty zone-facts list
// straight through; #537's Now screen is what exercises the resolve leg.
class StatusViewModel(
    private val rankPanesFn: suspend (nowMs: Long) -> List<MobileRankedPane>,
    /** The open chip's device-local store (`PanePrefs`, surface-keyed to
     * [MobileSurface.STATUS]) — injected like [rankPanesFn] so a JVM test
     * needs no DataStore. */
    private val readExpandedFn: suspend () -> String? = { null },
    private val writeExpandedFn: suspend (String?) -> Unit = {},
    private val queueDepthFn: suspend () -> UInt? = { null },
    private val apiVersionFn: suspend () -> UInt? = { null },
    private val syncFactsFn: suspend () -> MobileSyncFacts? = { null },
) : ViewModel() {

    private val _state = MutableStateFlow<StatusState>(StatusState.Loading)
    val state: StateFlow<StatusState> = _state.asStateFlow()

    /** Which quiet chip is open — one pane key or none, loaded once with
     * the first rank and written through [toggleExpanded]. Here, never in a
     * `remember {}`: the recorded fold/unfold defect.
     *
     * A stored key that no longer ranks is *kept*, not pruned — it simply
     * matches no chip, so nothing is open. That is `PaneCollapse.write`'s
     * own resurrection instinct: a pane that has gone quiet for now may be
     * back on the next rank, and the reader's choice should survive it. */
    private val _expandedKey = MutableStateFlow<String?>(null)
    val expandedKey: StateFlow<String?> = _expandedKey.asStateFlow()
    private var expandedKeyLoaded = false

    /** The one failure line this screen owns — `TriageViewModel`'s
     * `statusLine` shape, cleared by the next load that lands. */
    private val _statusLine = MutableStateFlow<String?>(null)
    val statusLine: StateFlow<String?> = _statusLine.asStateFlow()

    suspend fun load(nowMs: Long) {
        try {
            _state.value = StatusState.Loaded(
                panes = rankPanesFn(nowMs),
                rankedAtMs = nowMs,
                queueDepth = queueDepthFn(),
                apiVersion = apiVersionFn(),
                syncFacts = syncFactsFn(),
            )
            _statusLine.value = null
            if (!expandedKeyLoaded) {
                _expandedKey.value = readExpandedFn()
                expandedKeyLoaded = true
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

    /** Opens one chip's detail, or shuts it if it was already the open one.
     * Single selection is the state's shape, not a rule applied on top of
     * it: there is one key, so opening a second chip closes the first with
     * nothing to enforce. */
    suspend fun toggleExpanded(pane: MobileRankedPane) {
        val next = if (_expandedKey.value == pane.paneKey) null else pane.paneKey
        _expandedKey.value = next
        writeExpandedFn(next)
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
                readExpandedFn = { PanePrefs.readExpanded(appContext, MobileSurface.STATUS) },
                writeExpandedFn = { paneKey ->
                    PanePrefs.writeExpanded(appContext, MobileSurface.STATUS, paneKey)
                },
                queueDepthFn = { core().queueDepth() },
                apiVersionFn = { core().apiVersion() },
                syncFactsFn = { SyncHistoryStore.load(appContext) },
            )
        }

        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
