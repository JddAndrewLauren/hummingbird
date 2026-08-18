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
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import net.twinion.hummingbird.core.CoreHolder
import net.twinion.hummingbird.skills.MicrotaskRunner
import net.twinion.hummingbird.sync.SyncWorker
import uniffi.hummingbird_ffi_mobile.MobileSkillRunState
import uniffi.hummingbird_ffi_mobile.skillRunIdle

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
 */
class MicrotaskViewModel(
    private val runFn: (itemId: String, replace: Boolean, grain: Long?, model: String?) -> Flow<MobileSkillRunState>,
    private val syncFn: suspend () -> Unit,
) : ViewModel() {

    private val _run = MutableStateFlow<MobileSkillRunState>(skillRunIdle())
    val run: StateFlow<MobileSkillRunState> = _run.asStateFlow()

    /** The in-flight run's lock — a duplicate tap while one is streaming is
     * a no-op, the same rule the core's own reducer applies (belt AND
     * braces: the reducer alone cannot stop a second network request from
     * being started in the first place). */
    private var job: Job? = null

    fun run(itemId: String, replace: Boolean, grain: Long?, model: String?) {
        if (job?.isActive == true) return
        job = viewModelScope.launch {
            runFn(itemId, replace, grain, model).collect { state ->
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
            )
        }

        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
