package net.twinion.hummingbird.skills

import android.content.Context
import uniffi.hummingbird_ffi_mobile.backendAutoSelection
import uniffi.hummingbird_ffi_mobile.resolveBackendSelection

// #274's backend picker, persisted device-locally (#539) — the Kotlin twin
// of the web's `backend-selection.ts`. Plain `SharedPreferences`, not the
// Keystore-backed `core.TokenStore`: this is a rendering preference, not a
// credential, the same distinction `theme/theme.ts`'s preference draws on
// the web.
//
// **The registry itself stays a plain Kotlin list of ids** — this slice's
// one entry, `"cloud"` — never a decision. What IS a decision, sunk to the
// core and read through here rather than re-derived: the degrade-to-Auto
// rule ([resolveBackendSelection]) and the tier fallback
// ([uniffi.hummingbird_ffi_mobile.fallbackBackendId], read by
// [net.twinion.hummingbird.MicrotaskViewModel] directly, not through this
// object).
object BackendPreference {
    /** Declared order is the order Auto would walk. A future on-device or
     * home-runner tier appends here, exactly `backend-registry.ts`'s own
     * append-only rule. */
    val REGISTRY: List<String> = listOf("cloud")

    private const val PREFS_FILE = "hummingbird-preferences"
    private const val KEY_BACKEND_SELECTION = "backend_selection"

    private fun prefs(context: Context) =
        context.getSharedPreferences(PREFS_FILE, Context.MODE_PRIVATE)

    /** Auto when nothing is stored, or when the stored id no longer names a
     * registered entry — [resolveBackendSelection]'s own rule, never
     * re-derived here. */
    fun read(context: Context): String =
        resolveBackendSelection(prefs(context).getString(KEY_BACKEND_SELECTION, null), REGISTRY)

    fun write(context: Context, selection: String) {
        prefs(context).edit().putString(KEY_BACKEND_SELECTION, selection).apply()
    }

    /** The sentinel selection value — never a registered id. */
    val AUTO: String get() = backendAutoSelection()
}
