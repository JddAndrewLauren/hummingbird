package net.twinion.hummingbird.theme

// Theme preference (#535) — the phone's own copy of web's `theme/theme.ts`
// vocabulary. Deliberately **not** sunk to `hummingbird-core`: ADR-0025's
// verdict table already draws this line for `frontier-prefs.ts` ("view
// prefs" — device-local, no cross-client agreement needed), and a theme
// choice is the same kind of fact. `"system"` is a preference, not a
// theme: it resolves against the OS setting every time that setting
// changes, exactly as the web's own doc says.
enum class ThemePreference {
    SYSTEM,
    LIGHT,
    DARK,
}

/** What `HummingbirdTheme`'s `darkTheme: Boolean` should read, given the
 * preference and whatever `isSystemInDarkTheme()` currently says — the
 * Kotlin twin of `theme.ts`'s `resolveTheme`. */
fun resolveDarkTheme(preference: ThemePreference, systemPrefersDark: Boolean): Boolean = when (preference) {
    ThemePreference.SYSTEM -> systemPrefersDark
    ThemePreference.LIGHT -> false
    ThemePreference.DARK -> true
}
