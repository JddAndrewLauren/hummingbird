package net.twinion.hummingbird.wear.ui.theme

import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.em
import androidx.wear.compose.material3.Typography
import net.twinion.hummingbird.ui.theme.FigtreeFamily
import net.twinion.hummingbird.ui.theme.SpaceGroteskFamily
import net.twinion.hummingbird.ui.theme.SpaceMonoFamily

// The design system's three families on Wear M3's type scale: Space Grotesk
// for display and titles, Figtree for body and labels, Space Mono for the
// one meta style — the system's signature, "data the system computed" — as
// on the phone (`app/.../ui/theme/Type.kt`). Wear M3's own sizes and line
// heights are kept: they are tuned for a 1.2-inch round display, which the
// phone scale was never measured against, so only the families (and the
// mono meta tracking) are the brand's here.

private val WearBase = Typography(defaultFontFamily = FigtreeFamily)

val WearTypography: Typography = WearBase.copy(
    displayLarge = WearBase.displayLarge.copy(fontFamily = SpaceGroteskFamily, fontWeight = FontWeight.Bold),
    displayMedium = WearBase.displayMedium.copy(fontFamily = SpaceGroteskFamily, fontWeight = FontWeight.Bold),
    displaySmall = WearBase.displaySmall.copy(fontFamily = SpaceGroteskFamily, fontWeight = FontWeight.Bold),
    titleLarge = WearBase.titleLarge.copy(fontFamily = SpaceGroteskFamily, fontWeight = FontWeight.Bold),
    titleMedium = WearBase.titleMedium.copy(fontFamily = SpaceGroteskFamily, fontWeight = FontWeight.SemiBold),
    titleSmall = WearBase.titleSmall.copy(fontFamily = SpaceGroteskFamily, fontWeight = FontWeight.SemiBold),
    // The mono meta style: uppercase is the caller's, the tracking is the
    // token's (`--tracking-meta`, +0.08em).
    labelSmall = WearBase.labelSmall.copy(fontFamily = SpaceMonoFamily, letterSpacing = 0.08.em),
)
