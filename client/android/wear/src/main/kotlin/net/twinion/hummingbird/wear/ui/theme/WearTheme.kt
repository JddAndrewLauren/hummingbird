package net.twinion.hummingbird.wear.ui.theme

import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.wear.compose.material3.ColorScheme
import androidx.wear.compose.material3.MaterialTheme
import net.twinion.hummingbird.ui.theme.BorderDefaultDark
import net.twinion.hummingbird.ui.theme.BorderSubtleDark
import net.twinion.hummingbird.ui.theme.CrimsonDark
import net.twinion.hummingbird.ui.theme.Ember200
import net.twinion.hummingbird.ui.theme.Ember500
import net.twinion.hummingbird.ui.theme.Ember600
import net.twinion.hummingbird.ui.theme.Ember900
import net.twinion.hummingbird.ui.theme.Ink300
import net.twinion.hummingbird.ui.theme.Ink400
import net.twinion.hummingbird.ui.theme.Ink700
import net.twinion.hummingbird.ui.theme.Ink800
import net.twinion.hummingbird.ui.theme.Ink900
import net.twinion.hummingbird.ui.theme.Ink950
import net.twinion.hummingbird.ui.theme.Sand100
import net.twinion.hummingbird.ui.theme.StatusDangerBgDark
import net.twinion.hummingbird.ui.theme.StatusInfoFgDark

// The Hummingbird theme on Wear Material 3: the phone's dark `ColorScheme`
// (`app/.../ui/theme/Theme.kt`'s `DarkColors`) re-said in Wear M3's slot
// vocabulary, and nothing else — the watch is always dark (a round AMOLED
// display has no light mode worth drawing), so there is one scheme, no
// `LocalHbDark`, and every band/status pair reads its `dark = true` arm.
//
// **Every colour here is a `:brand` constant.** No `Color(0x…)` literal
// appears anywhere under `wear/src/main` — `WearThemeStructuralTest` pins
// that — so a token that moves in the design mirror moves here through the
// phone's `ColorTokenDriftTest` and `Color.kt`, never through a second copy.
//
// Mapping notes, where Wear M3's slots differ from phone M3's:
//  - Wear has no `surface`/`surfaceVariant`; its three `surfaceContainer*`
//    steps take the phone's card (`Ink800`), quiet (`Ink900`) and raised
//    (`Ink700`) surfaces respectively.
//  - `primaryDim`/`secondaryDim` are Wear's pressed/disabled tints — one
//    ramp step off the base, as the design README's press rule says.
//  - `--accent` (ember-500) is `primary`; ember is never a background
//    wash, so no ember-tinted surface slot exists here either.

private val WearColors = ColorScheme(
    primary = Ember500, // --accent
    primaryDim = Ember600,
    primaryContainer = Ember900,
    onPrimary = Color.White, // --on-accent
    onPrimaryContainer = Ember200,
    secondary = Ink300, // --text-secondary (dark)
    secondaryDim = Ink400,
    secondaryContainer = Ink700,
    onSecondary = Ink950,
    onSecondaryContainer = Sand100,
    tertiary = StatusInfoFgDark,
    tertiaryDim = StatusInfoFgDark,
    tertiaryContainer = Ink700,
    onTertiary = Ink950,
    onTertiaryContainer = Sand100,
    surfaceContainerLow = Ink900, // --surface-quiet (dark)
    surfaceContainer = Ink800, // --surface-card (dark)
    surfaceContainerHigh = Ink700,
    onSurface = Sand100, // --text-primary (dark)
    onSurfaceVariant = Ink300, // --text-secondary (dark)
    outline = BorderDefaultDark,
    outlineVariant = BorderSubtleDark,
    background = Ink950, // --surface-page (dark)
    onBackground = Sand100,
    error = CrimsonDark, // --status-danger-fg (dark)
    errorDim = CrimsonDark,
    errorContainer = StatusDangerBgDark,
    onError = Ink950,
    onErrorContainer = CrimsonDark,
)

@Composable
fun WearTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = WearColors,
        typography = WearTypography,
        content = content,
    )
}
