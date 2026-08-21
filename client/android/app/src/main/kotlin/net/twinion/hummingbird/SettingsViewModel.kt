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
import uniffi.hummingbird_ffi_mobile.MobileBindingRecord
import uniffi.hummingbird_ffi_mobile.MobileCalendarList
import uniffi.hummingbird_ffi_mobile.MobileCalendarSelection
import uniffi.hummingbird_ffi_mobile.MobileDeadLetterRecord
import uniffi.hummingbird_ffi_mobile.MobileSetBindingException
import uniffi.hummingbird_ffi_mobile.deadLetterHeading

/** Settings' own read of the seam (#535/M4) — bindings, the dead-letter
 * journal and the outbound queue's depth. Token entry/forget, the theme
 * preference, and the sync card's `lastSyncOutcomeKind`/`lastSyncAtMs` are
 * `AppRoot`'s state, not this class's — see `SettingsScreen.kt`'s own doc
 * for why the sync card in particular moved there (round-1 review, #535):
 * a `viewModel()` here is rebuilt every time its `NavBackStackEntry` is
 * left and re-entered, so state that lived only here would forget the
 * app's real sync history on every visit. */
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
// and the dead-letter rows' backing read. **This class decides nothing
// about a binding write's outcome or a dead-letter's heading.**
// `deadLetterHeadingText` arrives applied from
// `hummingbird_core::decisions::settings::dead_letter_heading` — no
// Kotlin-side classification, and `SettingsScreenStructuralTest` reads
// this file (and `SettingsScreen.kt`) to keep it that way.
//
// The injected-fn constructor is the house shape (`RulesViewModel`'s own
// doc): a plain JVM test can drive the control flow with no host
// `.so`/keystore in the process. That doc understates it a little here —
// `RulesViewModelTest`'s own fakes never actually reach `canSubmitCapture`
// — and `deadLetterHeadingFn` covers the same ground for
// `deadLetterHeading`: it is never called directly from a method body,
// precisely so a JVM test never touches the native library at all — the
// underlying decision is `hummingbird_core::decisions::settings`'s own
// test to own regardless.
class SettingsViewModel(
    private val fetchFn: suspend () -> SettingsRead,
    private val setBindingFn: suspend (key: String, value: String, nowMs: Long) -> Unit,
    private val deadLetterHeadingFn: (UInt) -> String = ::deadLetterHeading,
    private val listCalendarsFn: suspend () -> MobileCalendarList = { MobileCalendarList("no_credential", emptyList()) },
    private val readSelectionsFn: suspend () -> List<MobileCalendarSelection> = { emptyList() },
    private val writeSelectionsFn: suspend (List<MobileCalendarSelection>) -> Unit = {},
) : ViewModel() {

    private val _bindings = MutableStateFlow<List<MobileBindingRecord>?>(null)
    val bindings: StateFlow<List<MobileBindingRecord>?> = _bindings.asStateFlow()

    private val _deadLetters = MutableStateFlow<List<MobileDeadLetterRecord>>(emptyList())
    val deadLetters: StateFlow<List<MobileDeadLetterRecord>> = _deadLetters.asStateFlow()

    private val _queueDepth = MutableStateFlow(0u)
    val queueDepth: StateFlow<UInt> = _queueDepth.asStateFlow()

    /** The picker's options, or `null` before any list attempt. A failed
     * or credential-less list is kept as its own `kind` rather than
     * flattened to `null`: the picker's rule is that a bad list leaves the
     * options as they stand (`CalendarHostCore::list_calendars`'s own doc),
     * and "we asked and were refused" is a different sentence from "we have
     * not asked". */
    private val _calendars = MutableStateFlow<MobileCalendarList?>(null)
    val calendars: StateFlow<MobileCalendarList?> = _calendars.asStateFlow()

    /** Which calendars this device polls — the persisted selection, which
     * is the picker's checked state. Held here rather than re-read per
     * frame so a tap is visible before the store round-trips. */
    private val _calendarSelections = MutableStateFlow<List<MobileCalendarSelection>>(emptyList())
    val calendarSelections: StateFlow<List<MobileCalendarSelection>> =
        _calendarSelections.asStateFlow()

    /** The last binding write's failure, matched by key — `bindings.ts`'s
     * `bindingWriteError` own reasoning: a stale failure from a DIFFERENT
     * binding must never bleed onto this row. `null` on success or before
     * any write. */
    private val _bindingError = MutableStateFlow<Pair<String, String>?>(null)
    val bindingError: StateFlow<Pair<String, String>?> = _bindingError.asStateFlow()

    /** The dead-letter affordance's heading, off the real count — never a
     * fixed "1 edit didn't apply" string. */
    fun deadLetterHeadingText(): String = deadLetterHeadingFn(_deadLetters.value.size.toUInt())

    suspend fun load() {
        val read = fetchFn()
        _bindings.value = read.bindings
        _deadLetters.value = read.deadLetters
        _queueDepth.value = read.queueDepth
    }

    /** Reloads the picker: the persisted selection first (it renders even
     * with no credential), then the option list.
     *
     * `keepOptionsOnFailure` is the caller's read of whether the lane is
     * still armed. A failed list on an armed device (offline, or the
     * authority unreachable) must leave the options as they stand —
     * `CalendarHostCore::list_calendars`'s own rule, and what the web does
     * by posting nothing on a bad answer. A device that has been refused or
     * disconnected passes `false`, so the options it can no longer read
     * back do go away. */
    suspend fun loadCalendars(keepOptionsOnFailure: Boolean) {
        _calendarSelections.value = readSelectionsFn()
        val listed = listCalendarsFn()
        if (listed.kind == "ok" || _calendars.value == null || !keepOptionsOnFailure) {
            _calendars.value = listed
        }
    }

    /** Adds or removes one calendar from the polled set, persisting it and
     * pushing it through the seam. A newly-added calendar keeps the
     * standard horizon; the long horizon is the Vacation pane's own
     * business (#121) and no picker gesture here sets it. */
    suspend fun toggleCalendar(id: String) {
        val current = _calendarSelections.value
        val next = if (current.any { it.id == id }) {
            current.filterNot { it.id == id }
        } else {
            current + MobileCalendarSelection(id = id, longHorizon = false)
        }
        _calendarSelections.value = next
        writeSelectionsFn(next)
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
                listCalendarsFn = { core().listCalendars() },
                readSelectionsFn = { CalendarPrefs.readSelections(context.applicationContext) },
                writeSelectionsFn = { selections ->
                    CalendarPrefs.writeSelections(context.applicationContext, selections)
                    // The seam is told immediately, not at the next launch,
                    // and it polls on being told: the next timer tick may be
                    // fifteen minutes away, and until it lands the panes
                    // would answer off a snapshot taken over the *previous*
                    // selection.
                    core().setCalendarSelections(selections, System.currentTimeMillis())
                },
            )
        }

        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
