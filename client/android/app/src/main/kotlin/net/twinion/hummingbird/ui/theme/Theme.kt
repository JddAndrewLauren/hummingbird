package net.twinion.hummingbird.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

// The Hummingbird MaterialTheme: the design system's semantic aliases
// (tokens/colors.css) mapped onto Material 3's slots, per the grilling
// decision on #141 — native Material components everywhere, themed by
// token; dynamic color deliberately off (the palette is the brand, sampled
// from the app icon, not the wallpaper).
//
// Mapping notes, where the two vocabularies differ:
//  - `--accent` (ember-500) is `primary`; ember is never a background
//    wash, so no ember-tinted `surface*` slot exists.
//  - `--surface-quiet`/`--surface-sunken` land on `surfaceVariant` /
//    `surfaceContainerHighest`; cards are plain `surface`.
//  - `error` carries `--status-danger-*`.

private val LightColors = lightColorScheme(
    primary = Ember500, // --accent
    onPrimary = Color.White, // --on-accent
    primaryContainer = Ember50, // --accent-quiet
    onPrimaryContainer = Ember900,
    secondary = Ink600,
    onSecondary = Color.White,
    secondaryContainer = Sand200,
    onSecondaryContainer = Ink900,
    tertiary = Sky600, // --status-info-fg
    onTertiary = Color.White,
    tertiaryContainer = Sky100,
    onTertiaryContainer = Ink900,
    background = Sand50, // --surface-page
    onBackground = Ink900, // --text-primary
    surface = Color.White, // --surface-card
    onSurface = Ink900,
    surfaceVariant = Sand100, // --surface-quiet
    onSurfaceVariant = Ink500, // --text-secondary
    surfaceContainerHighest = Sand200, // --surface-sunken
    outline = BorderDefaultLight, // --border-default
    outlineVariant = Sand300, // --border-subtle
    error = Crimson600, // --status-danger-fg
    onError = Color.White,
    errorContainer = Crimson100, // --status-danger-bg
    onErrorContainer = Crimson600,
    inverseSurface = Ink900, // --surface-inverse
    inverseOnSurface = Sand50, // --text-inverse
    scrim = Ink900, // --surface-scrim's base color; alpha applied at use
)

private val DarkColors = darkColorScheme(
    primary = Ember500, // --accent (same anchor both modes)
    onPrimary = Color.White,
    primaryContainer = Ember900,
    onPrimaryContainer = Ember200,
    secondary = Ink300, // --text-secondary (dark)
    onSecondary = Ink950,
    secondaryContainer = Ink700,
    onSecondaryContainer = Sand100,
    tertiary = Color(0xFF5CB6D8), // --status-info-fg (dark)
    onTertiary = Ink950,
    tertiaryContainer = Ink700,
    onTertiaryContainer = Sand100,
    background = Ink950, // --surface-page (dark)
    onBackground = Sand100, // --text-primary (dark)
    surface = Ink800, // --surface-card (dark)
    onSurface = Sand100,
    surfaceVariant = Ink900, // --surface-quiet's 4% tint, on the card base
    onSurfaceVariant = Ink300,
    surfaceContainerHighest = SurfaceSunkenDark, // --surface-sunken (dark)
    outline = Color(0x1FFFFFFF), // --border-default (dark): white 12%
    outlineVariant = Color(0x12FFFFFF), // --border-subtle (dark): white 7%
    error = CrimsonDark, // --status-danger-fg (dark)
    onError = Ink950,
    errorContainer = Color(0x38B3261E), // --status-danger-bg (dark)
    onErrorContainer = CrimsonDark,
    inverseSurface = Sand100,
    inverseOnSurface = Ink900,
    scrim = Color.Black,
)

@Composable
fun HummingbirdTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    MaterialTheme(
        colorScheme = if (darkTheme) DarkColors else LightColors,
        typography = Typography,
        content = content,
    )
}
