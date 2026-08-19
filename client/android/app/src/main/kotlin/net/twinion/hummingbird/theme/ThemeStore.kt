package net.twinion.hummingbird.theme

import android.content.Context

// Where the theme preference rests — plain `SharedPreferences`, never the
// Android Keystore `TokenStore` uses: a theme choice carries no secret,
// unlike a device token (CLAUDE.md's credential-blast-radius rule only
// binds material that can act on the authority).
object ThemeStore {
    private const val PREFS_FILE = "hummingbird-preferences"
    private const val KEY_THEME = "theme_preference"

    private fun prefs(context: Context) =
        context.getSharedPreferences(PREFS_FILE, Context.MODE_PRIVATE)

    /** The stored preference, or [ThemePreference.SYSTEM] if none has ever
     * been saved — the same default `theme.ts`'s `readThemePreference`
     * falls back to for a missing or unrecognised stored value. */
    fun load(context: Context): ThemePreference {
        val raw = prefs(context).getString(KEY_THEME, null)
        return ThemePreference.entries.firstOrNull { it.name == raw } ?: ThemePreference.SYSTEM
    }

    fun save(context: Context, preference: ThemePreference) {
        prefs(context).edit().putString(KEY_THEME, preference.name).apply()
    }
}
