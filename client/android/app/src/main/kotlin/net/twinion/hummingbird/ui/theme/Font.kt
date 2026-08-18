package net.twinion.hummingbird.ui.theme

import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import net.twinion.hummingbird.R

// The brand's bundled typefaces (#528, ADR-0026): static OFL TTFs fetched
// from Google Fonts' own static-instance CDN (never a variable font, never
// a runtime GMS font-provider download) and committed under res/font/ —
// offline-safe, no network dependency at render time. Their OFL licenses
// live at app/licenses/fonts/, not res/font/ itself: aapt only accepts font
// and font-family-XML files inside a `font/` resource directory, so a
// license .txt there would fail the build.
//
// The family-name constants exist only for TypeTokenDriftTest (ADR-0026's
// companion to ColorTokenDriftTest): Compose's FontFamily carries no string
// name of its own, so this is where tokens/fonts.css's --font-display,
// --font-sans and --font-mono get a Kotlin-side value to pin against.
const val SPACE_GROTESK_FAMILY_NAME = "Space Grotesk"
const val FIGTREE_FAMILY_NAME = "Figtree"
const val SPACE_MONO_FAMILY_NAME = "Space Mono"

// Space Grotesk 400–700 — the full weight range the design token calls for,
// bundled regardless of which weights Type.kt currently reaches for.
val SpaceGroteskFamily = FontFamily(
    Font(R.font.space_grotesk_regular, FontWeight.Normal),
    Font(R.font.space_grotesk_medium, FontWeight.Medium),
    Font(R.font.space_grotesk_semibold, FontWeight.SemiBold),
    Font(R.font.space_grotesk_bold, FontWeight.Bold),
)

// Figtree — only the weights Type.kt actually uses (Normal for body,
// Medium for labelLarge): the design token's `@import` pulls the full
// 300..900 range for the web's arbitrary future use, but that range isn't
// "the weights actually used" the brief asks Android to bundle.
val FigtreeFamily = FontFamily(
    Font(R.font.figtree_regular, FontWeight.Normal),
    Font(R.font.figtree_medium, FontWeight.Medium),
)

// Space Mono 400/700 — the mono meta style's Normal weight plus the Bold
// the brief calls for bundling even though no TextStyle reaches for it yet.
val SpaceMonoFamily = FontFamily(
    Font(R.font.space_mono_regular, FontWeight.Normal),
    Font(R.font.space_mono_bold, FontWeight.Bold),
)
