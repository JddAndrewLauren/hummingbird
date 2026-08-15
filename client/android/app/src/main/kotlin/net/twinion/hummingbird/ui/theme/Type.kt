package net.twinion.hummingbird.ui.theme

import androidx.compose.material3.Typography
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp

// The design system's type scale on system families for M0: Space Grotesk
// (display), Figtree (body) and Space Mono (meta) are bundled as font
// resources in M1 alongside the mirrored Android UI kit — offline-safe
// files, not a GMS font-provider dependency. The sizes, line heights and
// the mono-meta letterspacing land now so no screen is ever built against
// a wrong scale.

val Typography = Typography(
    // Display/headings — Space Grotesk bold, tracking −0.022em at display
    // size (family arrives M1).
    headlineLarge = TextStyle(
        fontWeight = FontWeight.Bold,
        fontSize = 28.sp,
        lineHeight = 34.sp,
        letterSpacing = (-0.022).em,
    ),
    headlineSmall = TextStyle(
        fontWeight = FontWeight.Bold,
        fontSize = 22.sp,
        lineHeight = 28.sp,
        letterSpacing = (-0.022).em,
    ),
    titleMedium = TextStyle(
        fontWeight = FontWeight.SemiBold,
        fontSize = 17.sp,
        lineHeight = 24.sp,
    ),
    // Body — Figtree 15px / 1.55 (family arrives M1).
    bodyLarge = TextStyle(
        fontSize = 15.sp,
        lineHeight = 23.sp,
    ),
    bodyMedium = TextStyle(
        fontSize = 13.sp,
        lineHeight = 20.sp,
    ),
    // The mono meta style — the system's signature: 11px uppercase
    // +0.08em, "data the system computed". Monospace stands in for Space
    // Mono until M1.
    labelSmall = TextStyle(
        fontFamily = FontFamily.Monospace,
        fontWeight = FontWeight.Normal,
        fontSize = 11.sp,
        lineHeight = 16.sp,
        letterSpacing = 0.08.em,
    ),
    labelLarge = TextStyle(
        fontWeight = FontWeight.Medium,
        fontSize = 14.sp,
        lineHeight = 20.sp,
    ),
)
