package net.twinion.hummingbird

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.core.stringSetPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.first
import uniffi.hummingbird_ffi_mobile.MobileFrontierAxis

// M3/#530's frontier board: the two preferences `frontier-prefs.ts`'s own
// header (`client/web/src/screens/frontier-prefs.ts`) keeps device-local
// and persisted — the grouping axis and the per-column collapse set.
// Facet selection is the deliberate third that is NEVER persisted; see
// `FrontierFacetSelection`'s own doc in `NowViewModel.kt` for why.
//
// **Preferences DataStore, not `PushPrefs.kt`'s plain `SharedPreferences`.**
// Neither value here is a credential either (`PushPrefs.kt`'s own header
// has that argument, and it applies again): the axis name and the column
// labels currently shut are not secrets, and losing this store on a
// Keystore invalidation or a restore is a cosmetic re-derivation, not a
// dead-slot failure. The difference from `PushPrefs.kt` is call shape, not
// sensitivity — `NowViewModel`'s own doors are already `suspend`, so the
// coroutine-native store is the one with no `withContext(Dispatchers.IO)`
// wrapper needed at every call site.
private val Context.frontierPrefsStore: DataStore<Preferences> by preferencesDataStore(
    name = "hummingbird-frontier",
)

object FrontierPrefs {
    private val AXIS_KEY = stringPreferencesKey("axis")
    private val COLLAPSED_KEY = stringSetPreferencesKey("collapsed_columns")

    /** [MobileFrontierAxis.CONTEXT] (the default axis) is stored as key
     * ABSENCE, never as a value — `frontier-prefs.ts`'s own convention, so
     * an install that has never touched this store and one that has
     * explicitly picked Context read back identically. An unrecognised
     * stored value (a future axis this build predates) degrades to the
     * default rather than crashing the read. */
    suspend fun readAxis(context: Context): MobileFrontierAxis {
        val stored = context.frontierPrefsStore.data.first()[AXIS_KEY]
            ?: return MobileFrontierAxis.CONTEXT
        return runCatching { MobileFrontierAxis.valueOf(stored) }
            .getOrDefault(MobileFrontierAxis.CONTEXT)
    }

    suspend fun writeAxis(context: Context, axis: MobileFrontierAxis) {
        context.frontierPrefsStore.edit { prefs ->
            if (axis == MobileFrontierAxis.CONTEXT) {
                prefs.remove(AXIS_KEY)
            } else {
                prefs[AXIS_KEY] = axis.name
            }
        }
    }

    /** An empty collapse set is stored as key absence too, for the same
     * reason `writeAxis` above states. */
    suspend fun readCollapsedColumns(context: Context): Set<String> =
        context.frontierPrefsStore.data.first()[COLLAPSED_KEY] ?: emptySet()

    suspend fun writeCollapsedColumns(context: Context, collapsed: Set<String>) {
        context.frontierPrefsStore.edit { prefs ->
            if (collapsed.isEmpty()) {
                prefs.remove(COLLAPSED_KEY)
            } else {
                prefs[COLLAPSED_KEY] = collapsed
            }
        }
    }
}
