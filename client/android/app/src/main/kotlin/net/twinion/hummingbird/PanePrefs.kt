package net.twinion.hummingbird

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import java.io.IOException
import kotlinx.coroutines.flow.first
import net.twinion.hummingbird.ui.panes.CollapseOverride
import net.twinion.hummingbird.ui.panes.PaneCollapse
import uniffi.hummingbird_ffi_mobile.MobileSurface

// The pane collapse's device-local store (the pane-parity slice) — the
// web's `hb.questions.collapse` localStorage key, as a Preferences
// DataStore for exactly `FrontierPrefs.kt`'s reasons (that header carries
// the DataStore-vs-SharedPreferences argument; it applies verbatim). The
// map's semantics live in `ui/panes/PaneCollapse.kt`, pure and tested;
// this file is only the string round-trip and the degradation rule.
//
// **Per-surface namespacing**: Now and Status store under separate keys —
// the web's own `namespacedStorage` split (its Status key is suffixed
// `.status`), because the same question could one day rank on both
// surfaces and a collapse on one is not a preference about the other.
//
// A store that cannot be read or written is not an error a pane is allowed
// to fail on — `FrontierPrefs`' own rule: a failed read is the default
// (empty map), a failed write is silence.
private val Context.panePrefsStore: DataStore<Preferences> by preferencesDataStore(
    name = "hummingbird-panes",
)

internal object PanePrefs {
    private val NOW_KEY = stringPreferencesKey("collapse.now")
    private val STATUS_KEY = stringPreferencesKey("collapse.status")

    private fun keyFor(surface: MobileSurface) = when (surface) {
        MobileSurface.NOW -> NOW_KEY
        MobileSurface.STATUS -> STATUS_KEY
    }

    /** Which chip the Status quiet card has open, per surface.
     *
     * A key of its own rather than a [CollapseOverride]: an override is
     * band-stamped and many-keyed, which is right for "has the reader
     * overruled this pane's default, while it stays in this band" and wrong
     * for "which one thing is open". Stamped with a band, a chip's open
     * state would evaporate the moment its pane changed band — for no
     * reason the reader could see. */
    private val EXPANDED_NOW_KEY = stringPreferencesKey("expanded.now")
    private val EXPANDED_STATUS_KEY = stringPreferencesKey("expanded.status")

    private fun expandedKeyFor(surface: MobileSurface) = when (surface) {
        MobileSurface.NOW -> EXPANDED_NOW_KEY
        MobileSurface.STATUS -> EXPANDED_STATUS_KEY
    }

    private suspend fun <T> tolerating(access: suspend () -> T): T? = try {
        access()
    } catch (_: IOException) {
        null
    }

    suspend fun readCollapse(
        context: Context,
        surface: MobileSurface,
    ): Map<String, CollapseOverride> = readCollapse(context.panePrefsStore, surface)

    suspend fun writeCollapse(
        context: Context,
        surface: MobileSurface,
        map: Map<String, CollapseOverride>,
    ) = writeCollapse(context.panePrefsStore, surface, map)

    // The same doors against a DataStore handed in — `FrontierPrefs`' own
    // test seam, for the same no-Context-in-a-JVM-test reason.

    internal suspend fun readCollapse(
        store: DataStore<Preferences>,
        surface: MobileSurface,
    ): Map<String, CollapseOverride> =
        PaneCollapse.decode(tolerating { store.data.first()[keyFor(surface)] })

    suspend fun readExpanded(context: Context, surface: MobileSurface): String? =
        readExpanded(context.panePrefsStore, surface)

    suspend fun writeExpanded(context: Context, surface: MobileSurface, paneKey: String?) =
        writeExpanded(context.panePrefsStore, surface, paneKey)

    /** Nothing open is the key's *absence*, never a stored empty string —
     * `FrontierPrefs`' own encoding rule, so a default cannot rot into a
     * stale stored value. A blank reads as nothing open too, since a pane
     * key is never empty. */
    internal suspend fun readExpanded(
        store: DataStore<Preferences>,
        surface: MobileSurface,
    ): String? = tolerating { store.data.first()[expandedKeyFor(surface)] }?.takeIf(String::isNotBlank)

    internal suspend fun writeExpanded(
        store: DataStore<Preferences>,
        surface: MobileSurface,
        paneKey: String?,
    ) {
        tolerating {
            store.edit { prefs ->
                if (paneKey.isNullOrBlank()) {
                    prefs.remove(expandedKeyFor(surface))
                } else {
                    prefs[expandedKeyFor(surface)] = paneKey
                }
            }
        }
    }

    internal suspend fun writeCollapse(
        store: DataStore<Preferences>,
        surface: MobileSurface,
        map: Map<String, CollapseOverride>,
    ) {
        tolerating {
            store.edit { prefs ->
                if (map.isEmpty()) {
                    prefs.remove(keyFor(surface))
                } else {
                    prefs[keyFor(surface)] = PaneCollapse.encode(map)
                }
            }
        }
    }
}
