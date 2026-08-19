package net.twinion.hummingbird

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import net.twinion.hummingbird.core.CoreHolder
import net.twinion.hummingbird.skills.SkillRunner
import uniffi.hummingbird_ffi_mobile.ItemDetailRecord
import uniffi.hummingbird_ffi_mobile.ItemStepRecord
import uniffi.hummingbird_ffi_mobile.MobileGrillCompletion
import uniffi.hummingbird_ffi_mobile.MobileGrillTurn
import uniffi.hummingbird_ffi_mobile.MobileGrillTurnState

/** What the takeover is showing. [Loading] until the item's own read and
 * its draft (if any) have both landed — the same "resume waits for the
 * fresh answer" rule `useGrillTakeoverWiring.ts` documents, so a fresh
 * "Grill me" never races a `Question` phase built against an empty resume
 * that has not actually been checked yet. */
sealed interface GrillTakeoverState {
    data object Loading : GrillTakeoverState

    data class Ready(
        val item: ItemDetailRecord,
        /** This session's frozen Steps snapshot — [ItemDetailRecord.steps]
         * at the moment the takeover opened, never re-read afterward. The
         * same snapshot [confirm] sends as `sessionSteps`, so #354's
         * re-review guard has something fixed to compare the live steps
         * against. */
        val sessionSteps: List<ItemStepRecord>,
        val turn: MobileGrillTurnState,
        /** Every completed round so far — the wire body's own `turns`
         * array and the review card's transcript source. */
        val turns: List<MobileGrillTurn>,
        val confirming: Boolean,
        val completionError: String?,
    ) : GrillTakeoverState
}

/** The Grill takeover's own composition point (#355/#539, ADR-0023) — the
 * turn-asking lane over [SkillRunner.grillTurn] (a decided, applied result
 * per the core's `decisions::skills`, ADR-0025) and the review's Confirm/
 * Discard mutations, stitched into the one thing both mounts (item detail,
 * the Triage row) render against.
 *
 * **The draft auto-saves after every completed round**, never only on Back
 * — [answer] calls [saveDraftFn] the moment the round is complete, BEFORE
 * the next request goes out, which is what makes a backgrounded app lose
 * nothing (#539's own AC): the save already landed durably before the
 * process could be killed, not queued behind a lifecycle callback that
 * might not run.
 *
 * **Before, not after the answer comes back** (#565 review). Saving only
 * once a fresh `Question`/`Proposal` landed made durability depend on the
 * network: a decline, a hang, or a process death while `Asking` lost the
 * round the human had already typed, and re-opening resumed an older
 * transcript. The web saves on every change to its `turns`
 * (`useGrillTakeoverWiring.ts`'s continuous-save effect), which is the
 * contract this class states it follows. Nothing else here mutates `turns`
 * — `keepGrilling`/`retry`/`open` all re-ask with a list that is already
 * durable — so this one call site covers every round there is.
 */
class GrillTakeoverViewModel(
    private val itemDetailFn: suspend (itemId: String, nowMs: Long) -> ItemDetailRecord?,
    private val grillDraftFn: suspend (itemId: String) -> List<MobileGrillTurn>?,
    private val saveDraftFn: suspend (itemId: String, turns: List<MobileGrillTurn>, nowMs: Long) -> Unit,
    private val discardDraftFn: suspend (itemId: String, nowMs: Long) -> Unit,
    private val completeGrillFn: suspend (
        itemId: String,
        sessionSteps: List<ItemStepRecord>,
        completion: MobileGrillCompletion,
        nowMs: Long,
    ) -> String,
    private val grillTurn: (itemId: String, turns: List<MobileGrillTurn>) -> kotlinx.coroutines.flow.Flow<MobileGrillTurnState>,
) : ViewModel() {

    private val _state = MutableStateFlow<GrillTakeoverState>(GrillTakeoverState.Loading)
    val state: StateFlow<GrillTakeoverState> = _state.asStateFlow()

    /** The in-flight turn collection, if any — cancelled before a fresh one
     * starts, the same "one run at a time" rule [SkillRunner] itself keeps
     * via the duplicate-tap reducer, held one level up here because a
     * `Question`/`Proposal`/`Declined` re-ask is a NEW request each time,
     * not a duplicate tap on the same one. */
    private var askJob: Job? = null

    /** Opens the takeover over `itemId` — a no-op when it is already open
     * over that same item.
     *
     * **Why the guard matters more than it looks.** This `ViewModel`
     * survives Activity recreation (a Pixel Fold rotation/fold, the same
     * transition `ScreenStateRetentionTest` gates), but the Compose call
     * site does not: `GrillTakeoverScreen.kt`'s `LaunchedEffect(itemId)`
     * re-fires on every fresh composition, including one built after
     * recreation. Without this guard, a re-fire would reset [state] to
     * [GrillTakeoverState.Loading] and call [ask] again — losing whatever
     * turn/draft state had already streamed in, AND re-issuing a second,
     * billed `grill-me` request for a turn already in flight or already
     * answered. Idempotent by item id is what makes the guard survive a
     * genuine navigation to a DIFFERENT item, which must still open fresh. */
    fun open(itemId: String, nowMs: Long) {
        val current = _state.value
        if (current is GrillTakeoverState.Ready && current.item.id == itemId) return
        _state.value = GrillTakeoverState.Loading
        viewModelScope.launch {
            val item = itemDetailFn(itemId, nowMs) ?: return@launch
            val resumed = grillDraftFn(itemId)
            val startingTurns = resumed ?: emptyList()
            _state.value = GrillTakeoverState.Ready(
                item = item,
                sessionSteps = item.steps,
                // `MobileGrillTurnState.Idle` directly, not a call to the
                // real `grillTurnStarted(grillTurnIdle())` bindings — see
                // `MicrotaskViewModel._run`'s identical note. `ask`, called
                // right below, immediately overwrites this with the
                // injected `grillTurn` flow's own first emission (an
                // `Asking` state in production), so this is a placeholder
                // for the instant before that arrives, never itself the
                // "Started" transition.
                turn = MobileGrillTurnState.Idle,
                turns = startingTurns,
                confirming = false,
                completionError = null,
            )
            ask(itemId, startingTurns)
        }
    }

    /** Sends `turns` to the runner and streams the answer into [state] —
     * every `answer`/`keepGrilling`/`retry` gesture is this call with a
     * different `turns` list, `grill-me` being stateless. */
    private fun ask(itemId: String, turns: List<MobileGrillTurn>) {
        askJob?.cancel()
        askJob = viewModelScope.launch {
            grillTurn(itemId, turns).collect { turn ->
                val current = _state.value as? GrillTakeoverState.Ready ?: return@collect
                _state.value = current.copy(turn = turn, turns = turns)
            }
        }
    }

    /** One typed answer to the current question — persists the completed
     * round, then re-asks with the whole conversation threaded.
     *
     * The save runs in [viewModelScope] rather than inside [ask]'s own
     * [askJob], which the very next gesture cancels: a round already
     * answered must not lose its persistence to the request that follows
     * it. A never-answered session (`turns` empty here is impossible — this
     * appends one) still mints no draft, the rule
     * `useGrillTakeoverWiring.ts` states for its own save. */
    fun answer(itemId: String, text: String) {
        val current = _state.value as? GrillTakeoverState.Ready ?: return
        val question = current.turn as? MobileGrillTurnState.Question ?: return
        val turns = current.turns + MobileGrillTurn(question = question.question, answer = text)
        viewModelScope.launch { saveDraftFn(itemId, turns, System.currentTimeMillis()) }
        ask(itemId, turns)
    }

    /** "Keep grilling" from the review card — re-asks with the identical
     * transcript, since `grill-me` is stateless and nothing here decided to
     * add or drop a round. */
    fun keepGrilling(itemId: String) {
        val current = _state.value as? GrillTakeoverState.Ready ?: return
        _state.value = current.copy(completionError = null)
        ask(itemId, current.turns)
    }

    /** "Try again" from a decline — re-asks with the transcript already
     * threaded, never a fresh empty interview. */
    fun retry(itemId: String) {
        val current = _state.value as? GrillTakeoverState.Ready ?: return
        ask(itemId, current.turns)
    }

    /** Confirms against THIS session's frozen [GrillTakeoverState.Ready
     * .sessionSteps] — never a fresh read. A no-op while a previous confirm
     * is already in flight. Returns the minted Grill id on success and
     * leaves the caller to close the takeover; a failure sets
     * [GrillTakeoverState.Ready.completionError] and leaves the review
     * card standing, exactly `useGrillTakeoverWiring.ts`'s own contract. */
    suspend fun confirm(
        itemId: String,
        completion: MobileGrillCompletion,
        nowMs: Long,
    ): Boolean {
        val current = _state.value as? GrillTakeoverState.Ready ?: return false
        if (current.confirming) return false
        _state.value = current.copy(confirming = true, completionError = null)
        return try {
            completeGrillFn(itemId, current.sessionSteps, completion, nowMs)
            discardDraftFn(itemId, nowMs)
            // Reset even on success: the caller closes the takeover on a
            // `true` answer, but this class does not assume that happens —
            // leaving `confirming` stuck `true` would wrongly lock out a
            // second confirm if the same instance somehow stayed alive
            // (e.g. `onBack()` failing to navigate for an unrelated
            // reason). `completionError` stays cleared from the line
            // above.
            val after = _state.value as? GrillTakeoverState.Ready ?: current
            _state.value = after.copy(confirming = false)
            true
        } catch (error: Exception) {
            val after = _state.value as? GrillTakeoverState.Ready ?: current
            _state.value = after.copy(confirming = false, completionError = error.message ?: "Couldn't confirm.")
            false
        }
    }

    /** #356's explicit, confirmed "Discard" gesture — the caller confirms
     * with the human first; this only carries it out.
     *
     * `suspend`, not a fire-and-forget `launch` (#565 review): this
     * `ViewModel` is scoped to the takeover's own `NavBackStackEntry`, so
     * popping that entry clears the store and cancels [viewModelScope]. A
     * caller that popped immediately could cancel the delete mid-flight and
     * leave the supposedly discarded draft resumable — so the navigation
     * waits on this instead. */
    suspend fun discard(itemId: String, nowMs: Long) {
        discardDraftFn(itemId, nowMs)
    }

    companion object {
        fun create(context: Context): GrillTakeoverViewModel {
            val runner = SkillRunner.create(context)
            suspend fun core() = CoreHolder.get(context.applicationContext)
            return GrillTakeoverViewModel(
                itemDetailFn = { itemId, nowMs -> core().itemDetail(itemId, nowMs) },
                grillDraftFn = { itemId -> core().grillDraft(itemId) },
                saveDraftFn = { itemId, turns, nowMs -> core().saveGrillDraft(itemId, turns, nowMs) },
                discardDraftFn = { itemId, nowMs -> core().discardGrillDraft(itemId, nowMs) },
                completeGrillFn = { itemId, sessionSteps, completion, nowMs ->
                    core().completeGrill(itemId, sessionSteps, completion, nowMs)
                },
                grillTurn = { itemId, turns -> runner.grillTurn(itemId, turns) },
            )
        }

        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
