package net.twinion.hummingbird.ui.theme

import androidx.compose.material3.Typography
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp

// The design system's type scale, on the bundled brand families (#528):
// Space Grotesk (display), Figtree (body) and Space Mono (meta) — see
// Font.kt for how they're bundled. The sizes, line heights and the
// mono-meta letterspacing were pinned before the fonts arrived; only the
// `fontFamily` lines are new here.

val Typography = Typography(
    // Display/headings — Space Grotesk bold, tracking −0.022em at display
    // size.
    headlineLarge = TextStyle(
        fontFamily = SpaceGroteskFamily,
        fontWeight = FontWeight.Bold,
        fontSize = 28.sp,
        lineHeight = 34.sp,
        letterSpacing = (-0.022).em,
    ),
    headlineSmall = TextStyle(
        fontFamily = SpaceGroteskFamily,
        fontWeight = FontWeight.Bold,
        fontSize = 22.sp,
        lineHeight = 28.sp,
        letterSpacing = (-0.022).em,
    ),
    // The pane headline — the size the design handoff sets a Status card's
    // answer in (20/1.2, `--tracking-heading`). It was reached by name from
    // `StatusPanesExpanded.kt` long before it was defined here, so those
    // headlines were quietly rendering in Material's default Roboto; naming
    // it is what makes them Space Grotesk.
    titleLarge = TextStyle(
        fontFamily = SpaceGroteskFamily,
        fontWeight = FontWeight.Bold,
        fontSize = 20.sp,
        lineHeight = 24.sp,
        letterSpacing = (-0.014).em,
    ),
    titleMedium = TextStyle(
        fontFamily = SpaceGroteskFamily,
        fontWeight = FontWeight.SemiBold,
        fontSize = 17.sp,
        lineHeight = 24.sp,
    ),
    // Body — Figtree 15px / 1.55.
    bodyLarge = TextStyle(
        fontFamily = FigtreeFamily,
        fontWeight = FontWeight.Normal,
        fontSize = 15.sp,
        lineHeight = 23.sp,
    ),
    bodyMedium = TextStyle(
        fontFamily = FigtreeFamily,
        fontWeight = FontWeight.Normal,
        fontSize = 13.sp,
        lineHeight = 20.sp,
    ),
    // The mono meta style — the system's signature: 11px uppercase
    // +0.08em, "data the system computed".
    labelSmall = TextStyle(
        fontFamily = SpaceMonoFamily,
        fontWeight = FontWeight.Normal,
        fontSize = 11.sp,
        lineHeight = 16.sp,
        letterSpacing = 0.08.em,
    ),
    labelLarge = TextStyle(
        fontFamily = FigtreeFamily,
        fontWeight = FontWeight.Medium,
        fontSize = 14.sp,
        lineHeight = 20.sp,
    ),
)
