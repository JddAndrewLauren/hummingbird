package net.twinion.hummingbird.wear.questions

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.twinion.hummingbird.core.CoreHolder
import net.twinion.hummingbird.core.ZoneBridge
import uniffi.hummingbird_ffi_mobile.MobileQuestionRosterEntry
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileStandingQuestion
import uniffi.hummingbird_ffi_mobile.MobileSurface
import uniffi.hummingbird_ffi_mobile.MobileSyncFacts
import uniffi.hummingbird_ffi_mobile.MobileZoneFact
import uniffi.hummingbird_ffi_mobile.MobileZoneQuery
import uniffi.hummingbird_ffi_mobile.questionRoster

// The standing questions on the wrist (ADR-0039): the Now surface's six,
// **in the order the core's `rank_panes(Now)` returns them** — salience
// order, the same list the web's Now screen and the phone's Now panes draw
// — with the label each question carries on the core's roster (#714). This
// class decides nothing: which questions appear (a switched-off one is
// already absent, ADR-0034), in what order, with what band and answer, all
// arrive decided from the seam; it holds the list, the clock it was ranked
// against, and which rows the reader has opened this session. The wiring is
// `NowViewModel.create`'s exactly — zone queries through `ZoneBridge`, then
// the rank with no sync history (the reachability pane never sinks into Now).
//
// `Loaded.panes` is used as returned. No `sortedBy`, no filter; the
// structural test pins it, because a second total order here would be the
// exact drift the seam exists to prevent.
class QuestionsViewModel(
    private val paneZoneQueriesFn: suspend (nowMs: Long) -> List<MobileZoneQuery>,
    private val rankPanesFn: suspend (nowMs: Long, zoneFacts: List<MobileZoneFact>) -> List<MobileRankedPane>,
    private val rosterFn: suspend () -> List<MobileQuestionRosterEntry>,
) {
    /** The ranked list and the clock it was ranked against — one reading,
     * so the rows' words age with the rank and never against a second
     * clock. */
    data class Loaded(val panes: List<MobileRankedPane>, val nowMs: Long)

    private val _loaded = MutableStateFlow<Loaded?>(null)
    /** `null` until the first load completes — a real Loading state, never
     * an empty list standing in for one. */
    val loaded: StateFlow<Loaded?> = _loaded.asStateFlow()

    private val _expanded = MutableStateFlow<Set<String>>(emptySet())
    /** The `paneKey`s the reader has opened. Session-only, deliberately: a
     * collapse preference on a watch is not worth a DataStore. */
    val expanded: StateFlow<Set<String>> = _expanded.asStateFlow()

    private var labels: Map<MobileStandingQuestion, String> = emptyMap()

    suspend fun load(nowMs: Long) {
        if (labels.isEmpty()) {
            labels = rosterFn().associate { it.question to it.label }
        }
        val facts = ZoneBridge.resolve(paneZoneQueriesFn(nowMs))
        _loaded.value = Loaded(rankPanesFn(nowMs, facts), nowMs)
    }

    fun toggle(paneKey: String) {
        _expanded.value = if (paneKey in _expanded.value) _expanded.value - paneKey else _expanded.value + paneKey
    }

    /** The row's label: the roster's word for the question, and for a
     * per-series question the subject key after it — the phone's
     * `nowPaneLabel` rule for the race pane, over the roster's label rather
     * than a second literal. */
    fun label(pane: MobileRankedPane): String {
        val base = labels[pane.standingQuestion] ?: pane.standingQuestion.name.lowercase()
        return if (pane.standingQuestion == MobileStandingQuestion.RACE) "$base — ${pane.subjectKey}" else base
    }

    companion object {
        fun create(context: Context): QuestionsViewModel {
            val app = context.applicationContext
            return QuestionsViewModel(
                paneZoneQueriesFn = { nowMs -> CoreHolder.get(app).paneZoneQueries(MobileSurface.NOW, nowMs) },
                rankPanesFn = { nowMs, zoneFacts ->
                    CoreHolder.get(app).rankPanes(MobileSurface.NOW, nowMs, zoneFacts, MobileSyncFacts(null, null, null))
                },
                rosterFn = { questionRoster() },
            )
        }
    }
}
