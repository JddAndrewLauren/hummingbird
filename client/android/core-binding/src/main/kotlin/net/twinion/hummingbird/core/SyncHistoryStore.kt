package net.twinion.hummingbird.core

import android.content.Context
import uniffi.hummingbird_ffi_mobile.MobileSyncFacts

// The device's own authority-sync history (#536), persisted across
// restarts — plain `SharedPreferences`, `ThemeStore`'s own reasoning
// (nothing here is a secret, unlike `TokenStore`'s device token).
//
// **Why this lives in Kotlin and not in `hummingbird-core`.** No core
// decision reads sync history except the reachability pane
// (`MobileSyncFacts`'s own doc), and the web's equivalent
// (`QuestionSyncSnapshot`) is kept the same way — in the host's own store,
// never in the core. `AppRoot`'s `lastSyncOutcomeKind`/`lastSyncAtMs`
// (#535 review) already tracked the same two facts, but only in Compose
// state, which resets on process death; this store is what survives a
// cold start, so the reachability pane has something to reason over
// before this session's first cycle completes.
object SyncHistoryStore {
    private const val PREFS_FILE = "hummingbird-sync-history"
    private const val KEY_OUTCOME_KIND = "latest_outcome_kind"
    private const val KEY_INFORMATIVE_AT_MS = "latest_informative_at_ms"
    private const val KEY_SUCCESSFUL_AT_MS = "last_successful_at_ms"

    private fun prefs(context: Context) =
        context.getSharedPreferences(PREFS_FILE, Context.MODE_PRIVATE)

    /** The stored history, or every field `null` on a device that has
     * never completed an informative cycle — [reachability_answer]'s own
     * "never synced" gap reading. */
    fun load(context: Context): MobileSyncFacts {
        val stored = prefs(context)
        return MobileSyncFacts(
            latestOutcomeKind = stored.getString(KEY_OUTCOME_KIND, null),
            latestInformativeAtMs = readOptionalLong(stored, KEY_INFORMATIVE_AT_MS),
            lastSuccessfulAtMs = readOptionalLong(stored, KEY_SUCCESSFUL_AT_MS),
        )
    }

    /** Records one **informative** sync outcome (`isInformativeSyncOutcome`
     * — `AppRoot`'s own gate, never re-checked here: a `"skipped"`/`"busy"`
     * tick must never overwrite what this store says). `lastSuccessfulAtMs`
     * only advances on a `"completed"` kind, exactly the reachability
     * pane's own "landed" reading (`sync_outcome_class`). */
    fun recordInformative(context: Context, kind: String, atMs: Long) {
        prefs(context).edit().apply {
            putString(KEY_OUTCOME_KIND, kind)
            putLong(KEY_INFORMATIVE_AT_MS, atMs)
            if (kind == "completed") {
                putLong(KEY_SUCCESSFUL_AT_MS, atMs)
            }
        }.apply()
    }

    private fun readOptionalLong(stored: android.content.SharedPreferences, key: String): Long? =
        if (stored.contains(key)) stored.getLong(key, 0L) else null
}
