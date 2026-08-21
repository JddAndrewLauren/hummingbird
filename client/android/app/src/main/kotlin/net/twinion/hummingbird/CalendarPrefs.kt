package net.twinion.hummingbird

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringSetPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import java.io.IOException
import kotlinx.coroutines.flow.first
import uniffi.hummingbird_ffi_mobile.MobileCalendarSelection

// #564's two device-local calendar preferences: whether this device is
// opted in, and which calendars it polls. `FrontierPrefs.kt`'s store, its
// DataStore-over-SharedPreferences argument and its degradation rule apply
// here verbatim (that header carries all three); this file adds only the
// pair of keys and the one thing that is genuinely different about them.
//
// **What is different: neither of these is a credential, and this file is
// where that has to stay true.** The opt-in flag is a *preference*
// (`calendar/persistence.ts`'s own distinction, ported): it says the
// operator chose to connect, not that anything here can connect. The
// Google access token is minted in Rust, held in Rust for its ~1-hour life
// and re-minted on demand — it is never handed to Kotlin, so there is
// nothing for this store to be tempted by. The device token is
// `TokenStore`'s, and a second copy of it here would be a second
// credential lifecycle for no gain. **Nothing token-shaped may be added to
// this file**; `SettingsScreenStructuralTest` pins that.
//
// **Losing this store costs one Connect tap**, not data: the mirror on disk
// is `hummingbird-core`'s and survives, and a device that reads back
// `connected = false` simply polls nothing until the operator reconnects.
private val Context.calendarPrefsStore: DataStore<Preferences> by preferencesDataStore(
    name = "hummingbird-calendar",
)

object CalendarPrefs {
    private val CONNECTED_KEY = booleanPreferencesKey("connected")
    private val SELECTED_KEY = stringSetPreferencesKey("selected_calendars")
    private val LONG_HORIZON_KEY = stringSetPreferencesKey("long_horizon_calendars")

    private suspend fun <T> tolerating(access: suspend () -> T): T? = try {
        access()
    } catch (_: IOException) {
        null
    }

    suspend fun readConnected(context: Context): Boolean =
        readConnected(context.calendarPrefsStore)

    suspend fun writeConnected(context: Context, connected: Boolean) =
        writeConnected(context.calendarPrefsStore, connected)

    suspend fun readSelections(context: Context): List<MobileCalendarSelection> =
        readSelections(context.calendarPrefsStore)

    suspend fun writeSelections(context: Context, selections: List<MobileCalendarSelection>) =
        writeSelections(context.calendarPrefsStore, selections)

    // The same doors against a DataStore handed in — `FrontierPrefs`' own
    // test seam, for the same no-Context-in-a-JVM-test reason.

    internal suspend fun readConnected(store: DataStore<Preferences>): Boolean =
        tolerating { store.data.first()[CONNECTED_KEY] } ?: false

    internal suspend fun writeConnected(store: DataStore<Preferences>, connected: Boolean) {
        tolerating {
            store.edit { prefs ->
                // Absence is "never connected", so disconnecting removes
                // the key rather than writing `false` — the same
                // absence-is-the-default convention `FrontierPrefs` uses
                // for its own default axis.
                if (connected) prefs[CONNECTED_KEY] = true else prefs.remove(CONNECTED_KEY)
            }
        }
    }

    /** The picked calendars, each with the horizon it was picked under
     * (#121). Two sets rather than one encoded string: a calendar id is
     * opaque provider text that may contain any separator a hand-rolled
     * encoding would pick, and the long-horizon set is a strict subset of
     * the selected set, so membership is the whole representation. Order is
     * not preserved and is not meaningful — the poller iterates the
     * selection, it does not rank it — but the list is sorted here so two
     * reads of an unchanged store are equal, which is what lets the
     * `LaunchedEffect` keyed on it in Settings settle. */
    internal suspend fun readSelections(
        store: DataStore<Preferences>,
    ): List<MobileCalendarSelection> {
        val prefs = tolerating { store.data.first() } ?: return emptyList()
        val long = prefs[LONG_HORIZON_KEY].orEmpty()
        return prefs[SELECTED_KEY].orEmpty().sorted().map { id ->
            MobileCalendarSelection(id = id, longHorizon = id in long)
        }
    }

    internal suspend fun writeSelections(
        store: DataStore<Preferences>,
        selections: List<MobileCalendarSelection>,
    ) {
        tolerating {
            store.edit { prefs ->
                if (selections.isEmpty()) {
                    prefs.remove(SELECTED_KEY)
                    prefs.remove(LONG_HORIZON_KEY)
                } else {
                    prefs[SELECTED_KEY] = selections.map { it.id }.toSet()
                    val long = selections.filter { it.longHorizon }.map { it.id }.toSet()
                    if (long.isEmpty()) {
                        prefs.remove(LONG_HORIZON_KEY)
                    } else {
                        prefs[LONG_HORIZON_KEY] = long
                    }
                }
            }
        }
    }
}
