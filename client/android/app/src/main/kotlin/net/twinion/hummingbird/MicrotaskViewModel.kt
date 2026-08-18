package net.twinion.hummingbird

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import net.twinion.hummingbird.core.CoreHolder
import net.twinion.hummingbird.skills.BackendPreference
import net.twinion.hummingbird.skills.MicrotaskRunner
import net.twinion.hummingbird.sync.SyncWorker
import uniffi.hummingbird_ffi_mobile.MobileSkillRunState
import uniffi.hummingbird_ffi_mobile.declinedBackendFallback

/** The microtask affordance's own wiring (#273, landed on the phone at
 * #539): tap, stream, then ask for one sync cycle so the steps arrive
 * through the normal read path — `useMicrotaskWiring.ts`'s own contract.
 *
 * **This screen decides nothing about eligibility.** [ItemDetailScreen]
 * reads `ItemDetailRecord.microtaskAffordance` — the core's applied result
 * (ADR-0025) — for whether to offer the button at all and which of the two
 * gestures (break/rewrite) it is; this class only runs the tap that
 * follows and narrates what comes back. A decline is rendered verbatim,
 * never paraphrased, the same rule [GrillTakeoverViewModel] follows for
 * its own declines.
 *
 * **The backend selection (#274) reaches every run through here.** [run]
 * resolves `model` off the CURRENT [selection] and [BackendPreference
 * .modelFor] — never a caller-supplied literal — so Settings' picker
 * actually changes what a tap does, and [declinedFallbackId] /
 * [switchAndRetry] give a declined, unreachable pin the same one-tap
 * "switch tiers and retry" offer `useMicrotaskWiring.ts`'s
 * `declinedFallback` gives the web.
 */
class MicrotaskViewModel(
    private val runFn: (itemId: String, replace: Boolean, grain: Long?, model: String?) -> Flow<MobileSkillRunState>,
    private val syncFn: suspend () -> Unit,
    private val readSelectionFn: () -> String,
    private val writeSelectionFn: (String) -> Unit,
    private val modelForFn: (String) -> String?,
    /** [uniffi.hummingbird_ffi_mobile.declinedBackendFallback], injected for
     * the same off-native-process reason every other core call in this
     * class is. */
    private val declinedFallbackFn: (MobileSkillRunState, String, List<String>) -> String?,
    private val registryIds: List<String>,
) : ViewModel() {

    // `MobileSkillRunState.Idle` directly — never a call to the real
    // `skillRunIdle()` binding here, which would make constructing this
    // class (with any fakes injected) still require the native library to
    // be loaded. `Idle` carries no fields, so no uniffi crossing is needed
    // to produce it; `SkillsSeamTest`'s own reasoning for keeping every
    // *reducer* a plain constructor default applies the same way to this
    // one plain value.
    private val _run = MutableStateFlow<MobileSkillRunState>(MobileSkillRunState.Idle)
    val run: StateFlow<MobileSkillRunState> = _run.asStateFlow()

    private val _selection = MutableStateFlow(readSelectionFn())
    val selection: StateFlow<String> = _selection.asStateFlow()

    /** #274's one-tap fallback offer for the CURRENT [run] — `null`
     * whenever there is nothing to offer (see the uniffi door's own doc for
     * the four exclusions: not declined, Auto already tried everything, a
     * backend answered, or no token was ever sent).
     *
     * A `StateFlow`, not a plain getter: [switchAndRetry] writes
     * [selection] before the new run's first state lands, and a caller that
     * only collected [run] would read this one recomposition/emission
     * stale in that gap. Combining both is what makes "derived from run and
     * selection" a property Compose can actually subscribe to, rather than
     * a value that merely happens to be fresh whenever something else also
     * triggers a re-read. */
    val declinedFallbackId: StateFlow<String?> = combine(_run, _selection) { run, selection ->
        declinedFallbackFn(run, selection, registryIds)
    }.stateIn(viewModelScope, SharingStarted.Eagerly, null)

    /** The in-flight run's lock — a duplicate tap while one is streaming is
     * a no-op, the same rule the core's own reducer applies (belt AND
     * braces: the reducer alone cannot stop a second network request from
     * being started in the first place). */
    private var job: Job? = null

    /** What [run] was last asked to do — [switchAndRetry]'s own replay
     * target. Never the item id/replace/grain the CALLER might ask with
     * next; only this run's. */
    private var lastRequest: Request? = null

    private data class Request(val itemId: String, val replace: Boolean, val grain: Long?)

    fun run(itemId: String, replace: Boolean, grain: Long?) {
        startRun(Request(itemId, replace, grain), _selection.value)
    }

    /** The fallback button's one call: switches the device preference AND
     * retries the SAME request as the fallback tier, in a single call —
     * never two. `onSelectBackend(); run()` across two ticks would read a
     * stale [selection] before the write took effect; handing back one
     * function makes that miswiring unexpressible, the same reasoning
     * `useMicrotaskWiring.ts`'s own `onSwitchAndRun` states. */
    fun switchAndRetry() {
        val fallback = declinedFallbackId.value ?: return
        val request = lastRequest ?: return
        writeSelectionFn(fallback)
        _selection.value = fallback
        startRun(request, fallback)
    }

    private fun startRun(request: Request, selection: String) {
        if (job?.isActive == true) return
        lastRequest = request
        val model = modelForFn(selection)
        job = viewModelScope.launch {
            runFn(request.itemId, request.replace, request.grain, model).collect { state ->
                _run.value = state
                // Only on a terminal `Done` — never on progress, and never
                // on a schedule. The same "ask for one cycle, never poll"
                // rule `useMicrotaskWiring.ts` documents for its own
                // `triggerSyncManual` call.
                if (state is MobileSkillRunState.Done) syncFn()
            }
        }
    }

    companion object {
        fun create(context: Context): MicrotaskViewModel {
            val runner = MicrotaskRunner.create(context)
            return MicrotaskViewModel(
                runFn = { itemId, replace, grain, model -> runner.run(itemId, replace, grain, model) },
                syncFn = {
                    CoreHolder.get(context.applicationContext).run(
                        System.currentTimeMillis(),
                        SyncWorker.TRIGGER_PUSH,
                        false,
                        kotlin.random.Random.nextDouble(),
                    )
                },
                readSelectionFn = { BackendPreference.read(context) },
                writeSelectionFn = { selection -> BackendPreference.write(context, selection) },
                modelForFn = BackendPreference::modelFor,
                declinedFallbackFn = { state, selection, registryIds ->
                    declinedBackendFallback(state, selection, registryIds)
                },
                registryIds = BackendPreference.REGISTRY,
            )
        }

        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
