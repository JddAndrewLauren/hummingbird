package net.twinion.hummingbird.skills

import android.content.Context
import uniffi.hummingbird_ffi_mobile.backendAutoSelection
import uniffi.hummingbird_ffi_mobile.resolveBackendSelection

// #274's backend picker, persisted device-locally (#539) — the Kotlin twin
// of the web's `backend-selection.ts`/`backend-registry.ts`. Plain
// `SharedPreferences`, not the Keystore-backed `core.TokenStore`: this is a
// rendering preference, not a credential, the same distinction
// `theme/theme.ts`'s preference draws on the web.
//
// **The registry's entries stay plain Kotlin data** — this slice's one,
// the cloud runner — never a decision. What IS a decision, sunk to the core
// and read through here (or through
// [uniffi.hummingbird_ffi_mobile.declinedBackendFallback], read by
// [net.twinion.hummingbird.MicrotaskViewModel]) rather than re-derived: the
// degrade-to-Auto rule ([resolveBackendSelection]) and the tier fallback.
object BackendPreference {
    /** One registered tier — `backend-registry.ts`'s own `BackendEntry`,
     * Kotlin-side. `model` is fixed per entry, never chosen separately: a
     * future entry with a different model is a different entry. */
    data class Entry(val id: String, val label: String, val model: String?)

    /** Declared order is the order Auto would walk. #275/#276 append here,
     * exactly `backend-registry.ts`'s own append-only rule. */
    val ENTRIES: List<Entry> = listOf(Entry(id = "cloud", label = "Cloud runner", model = null))

    val REGISTRY: List<String> = ENTRIES.map { it.id }

    /** The `model` a run against `id` sends — `null` when `id` names no
     * registered entry, the same "a stale id has nothing to answer with"
     * reading [resolveBackendSelection] already gives a stale selection. */
    fun modelFor(id: String): String? = ENTRIES.find { it.id == id }?.model

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
