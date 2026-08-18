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
import net.twinion.hummingbird.core.NetworkStatus
import net.twinion.hummingbird.sync.SyncWorker
import uniffi.hummingbird_ffi_mobile.MobileBindingRecord
import uniffi.hummingbird_ffi_mobile.MobileDeadLetterRecord
import uniffi.hummingbird_ffi_mobile.MobileSetBindingException
import uniffi.hummingbird_ffi_mobile.MobileSyncStatusInput
import uniffi.hummingbird_ffi_mobile.MobileSyncStatusSummary
import uniffi.hummingbird_ffi_mobile.deadLetterHeading
import uniffi.hummingbird_ffi_mobile.isInformativeSyncOutcome
import uniffi.hummingbird_ffi_mobile.syncStatusSummary

/** Settings' own read of the seam (#535/M4) — bindings, the dead-letter
 * journal and the outbound queue's depth. Token entry/forget and the theme
 * preference are device-local and handled directly by `AppRoot`/
 * `SettingsScreen`, the same split `RulesScreen`'s own doc draws between
 * what needs a ViewModel and what does not. */
data class SettingsRead(
    val bindings: List<MobileBindingRecord>,
    val deadLetters: List<MobileDeadLetterRecord>,
    val queueDepth: UInt,
)

/** Mints one binding write's seed — the Kotlin twin of `useBindingsWiring
 * .ts`'s `mintBindingSeed`, and deliberately the identical spelling
 * (`"$key:binding:$nowMs"`): a binding write touches the `settings` row
 * `key` itself names, so retrying the identical intent (same key, same
 * `nowMs`) must reproduce the identical queue entry rather than enqueue a
 * second one — ADR-0007's seed-minting rule (#223), read the same way on
 * both clients. */
fun mintBindingSeed(key: String, nowMs: Long): String = "$key:binding:$nowMs"

// The Settings screen (#535/M4): bindings with their compare-and-set write,
// the sync-status card, and the dead-letter rows. **This class decides
// nothing about sync status.** `syncSummary` arrives applied from
// `hummingbird_core::decisions::settings::sync_status_summary`, reached
// through the free `syncStatusSummary` door — there is no Kotlin-side
// classification of what "stale"/"held"/"synced" mean, and
// `SettingsScreenStructuralTest` reads this file (and `SettingsScreen.kt`)
// to keep it that way.
//
// The injected-fn constructor is the house shape (`RulesViewModel`'s own
// doc): a plain JVM test can drive the control flow with no host
// `.so`/keystore/`ConnectivityManager` in the process. That doc understates
// it a little here — `RulesViewModelTest`'s own fakes never actually reach
// `canSubmitCapture`, and this class's own `*Fn` parameters below cover the
// same ground for `syncStatusSummary`/`isInformativeSyncOutcome`/
// `deadLetterHeading`: none of the three is ever called directly from a
// method body, precisely so a JVM test never touches the native library at
// all — the underlying decision is `hummingbird_core::decisions::settings`'
// own test to own regardless.
class SettingsViewModel(
    private val fetchFn: suspend () -> SettingsRead,
    private val setBindingFn: suspend (key: String, value: String, nowMs: Long) -> Unit,
    /** One sync cycle, answering the outcome's wire `kind` — the same
     * string `hummingbird_core::decisions::settings` classifies. */
    private val runFn: suspend (nowMs: Long) -> String,
    private val onlineFn: () -> Boolean,
    private val syncStatusSummaryFn: (MobileSyncStatusInput) -> MobileSyncStatusSummary = ::syncStatusSummary,
    private val isInformativeSyncOutcomeFn: (String) -> Boolean = ::isInformativeSyncOutcome,
    private val deadLetterHeadingFn: (UInt) -> String = ::deadLetterHeading,
) : ViewModel() {

    private val _bindings = MutableStateFlow<List<MobileBindingRecord>?>(null)
    val bindings: StateFlow<List<MobileBindingRecord>?> = _bindings.asStateFlow()

    private val _deadLetters = MutableStateFlow<List<MobileDeadLetterRecord>>(emptyList())
    val deadLetters: StateFlow<List<MobileDeadLetterRecord>> = _deadLetters.asStateFlow()

    private val _queueDepth = MutableStateFlow(0u)
    val queueDepth: StateFlow<UInt> = _queueDepth.asStateFlow()

    /** The last binding write's failure, matched by key — `bindings.ts`'s
     * `bindingWriteError` own reasoning: a stale failure from a DIFFERENT
     * binding must never bleed onto this row. `null` on success or before
     * any write. */
    private val _bindingError = MutableStateFlow<Pair<String, String>?>(null)
    val bindingError: StateFlow<Pair<String, String>?> = _bindingError.asStateFlow()

    private val _lastSyncOutcomeKind = MutableStateFlow<String?>(null)
    private val _lastSyncAtMs = MutableStateFlow<Long?>(null)

    /** The sync card's whole read, off this screen's own last-sync state.
     * `isInformativeSyncOutcomeFn` is what keeps a `"skipped"`/`"busy"`
     * tick from re-greening this card mid-outage — the same guard
     * `store/worker-client.ts` applies on the web side. */
    fun syncSummary(nowMs: Long): MobileSyncStatusSummary = syncStatusSummaryFn(
        MobileSyncStatusInput(
            online = onlineFn(),
            lastSyncOutcomeKind = _lastSyncOutcomeKind.value,
            lastSyncAtMs = _lastSyncAtMs.value,
            queueDepth = _queueDepth.value,
            nowMs = nowMs,
        ),
    )

    /** The dead-letter affordance's heading, off the real count — never a
     * fixed "1 edit didn't apply" string. */
    fun deadLetterHeadingText(): String = deadLetterHeadingFn(_deadLetters.value.size.toUInt())

    suspend fun load() {
        val read = fetchFn()
        _bindings.value = read.bindings
        _deadLetters.value = read.deadLetters
        _queueDepth.value = read.queueDepth
    }

    /** Sets one binding. The draft's worth-sending check is the screen's —
     * this trusts it and enqueues, `useBindingsWiring.ts`'s own split. */
    suspend fun setBinding(key: String, value: String, nowMs: Long) {
        try {
            setBindingFn(key, value, nowMs)
        } catch (error: MobileSetBindingException) {
            _bindingError.value = key to when (error) {
                is MobileSetBindingException.UnknownKey ->
                    "This build doesn't know that binding, so it wasn't saved."
                is MobileSetBindingException.WriteFailed -> error.detail
            }
            return
        }
        _bindingError.value = null
        load()
    }

    /** One sync cycle, off this screen's own cadence — `RulesViewModel
     * .save`'s own precedent for a screen driving a cycle independent of
     * `AppRoot`'s. Only an *informative* outcome overwrites the last-sync
     * state: a backed-off tick must never read as a fresh "Synced". */
    suspend fun sync(nowMs: Long) {
        val kind = runFn(nowMs)
        if (isInformativeSyncOutcomeFn(kind)) {
            _lastSyncOutcomeKind.value = kind
            _lastSyncAtMs.value = nowMs
        }
        load()
    }

    companion object {
        fun create(context: Context): SettingsViewModel {
            suspend fun core() = CoreHolder.get(context.applicationContext)
            return SettingsViewModel(
                fetchFn = {
                    val host = core()
                    SettingsRead(
                        bindings = host.bindings(),
                        deadLetters = host.deadLetters(),
                        queueDepth = host.queueDepth(),
                    )
                },
                setBindingFn = { key, value, nowMs ->
                    core().setBinding(mintBindingSeed(key, nowMs), key, value, nowMs)
                },
                runFn = { nowMs ->
                    core().run(nowMs, SyncWorker.TRIGGER_PUSH, false, Random.nextDouble()).kind
                },
                onlineFn = { NetworkStatus.isOnline(context.applicationContext) },
            )
        }

        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
