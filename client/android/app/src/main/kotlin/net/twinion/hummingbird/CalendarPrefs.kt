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

// #564's two device-local calendar preferences — whether this device was
// opted in, and which calendars it polled — **read once, for ADR-0040
// decision 8's migration, and then deleted.** Both are synced `settings`
// rows now (`hummingbird_core::calendar_prefs`), read and written through
// the core via `MobileTaskHost`; nothing here writes either key any more.
// `MainActivity`'s sync loop hands what it finds to the core's one-time
// seed (`MobileTaskHost.seedCalendarPrefs`) and, once the core answers
// anything but `NOT_YET`, clears them. `FrontierPrefs.kt`'s store, its
// DataStore-over-SharedPreferences argument and its degradation rule apply
// here verbatim (that header carries all three).
//
// **Neither of these is a credential, and this file is where that has to
// stay true.** The Google access token is minted in Rust, held in Rust for
// its ~1-hour life and re-minted on demand — it is never handed to Kotlin.
// The device token is `TokenStore`'s. **Nothing token-shaped may be added
// to this file**; `CalendarPrefsTest` pins that.
//
// **Losing this store costs one Connect tap**, not data: an unreadable
// store reads as nothing to migrate, and the operator connects again.
private val Context.calendarPrefsStore: DataStore<Preferences> by preferencesDataStore(
    name = "hummingbird-calendar",
)

object CalendarPrefs {
    private val CONNECTED_KEY = booleanPreferencesKey("connected")
    private val SELECTED_KEY = stringSetPreferencesKey("selected_calendars")
    private val LONG_HORIZON_KEY = stringSetPreferencesKey("long_horizon_calendars")

    /** The pre-#843 copy: the opt-in flag and the picked calendars, each with
     * the horizon it was picked under (the long-horizon set is a strict
     * subset of the selected one, so membership is the whole
     * representation — a calendar id is opaque provider text, and no
     * separator would be safe to encode with). */
    data class Local(val connected: Boolean, val selections: List<MobileCalendarSelection>)

    private suspend fun <T> tolerating(access: suspend () -> T): T? = try {
        access()
    } catch (_: IOException) {
        null
    }

    suspend fun readLocal(context: Context): Local? = readLocal(context.calendarPrefsStore)

    suspend fun clear(context: Context) = clear(context.calendarPrefsStore)

    // The same doors against a DataStore handed in — `FrontierPrefs`' own
    // test seam, for the same no-Context-in-a-JVM-test reason.

    /** `null` when there is nothing to migrate: neither key is present (a
     * device that never connected, or one already migrated), or the store
     * cannot be read. Sorted, so two reads of an unchanged store are equal. */
    internal suspend fun readLocal(store: DataStore<Preferences>): Local? {
        val prefs = tolerating { store.data.first() } ?: return null
        val selected = prefs[SELECTED_KEY]
        val connected = prefs[CONNECTED_KEY]
        if (selected == null && connected == null) return null
        val long = prefs[LONG_HORIZON_KEY].orEmpty()
        return Local(
            connected = connected == true,
            selections = selected.orEmpty().sorted().map { id ->
                MobileCalendarSelection(id = id, longHorizon = id in long)
            },
        )
    }

    /** Deletes every key — the migration's last step, on a settled seed. A
     * store that cannot be written fails silently: the next settled seed
     * is an `ADOPTED` no-op, and clears it then. */
    internal suspend fun clear(store: DataStore<Preferences>) {
        tolerating {
            store.edit { prefs ->
                prefs.remove(CONNECTED_KEY)
                prefs.remove(SELECTED_KEY)
                prefs.remove(LONG_HORIZON_KEY)
            }
        }
    }
}
