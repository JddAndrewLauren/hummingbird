package net.twinion.hummingbird.ui.theme

import androidx.compose.ui.graphics.Color

// The Hummingbird base ramps, verbatim from the design system's
// tokens/colors.css (mirrored at .claude/skills/hummingbird-design/) —
// sampled from the app icon: ember from the throat, ink from the dark
// plate, sand from the light plate, plume from the head feathers. Only the
// ramp values this skeleton's theme maps are carried; add the rest of a
// ramp when a screen needs it, never a value the CSS doesn't have.

val Ember50 = Color(0xFFFFF4EA)
val Ember200 = Color(0xFFFFC68F)
val Ember300 = Color(0xFFFEA354)
val Ember400 = Color(0xFFFE8A03)
val Ember500 = Color(0xFFEB6D06)
val Ember600 = Color(0xFFD45505)
val Ember700 = Color(0xFFC62704)
val Ember900 = Color(0xFF6E1E06)

val Ink100 = Color(0xFFE3E7EA)
val Ink200 = Color(0xFFC8CFD5)
val Ink300 = Color(0xFF9FAAB4)
val Ink400 = Color(0xFF6F7D89)
val Ink500 = Color(0xFF4C5A67)
val Ink600 = Color(0xFF384552)
val Ink700 = Color(0xFF2F3A45)
val Ink800 = Color(0xFF212A33)
val Ink900 = Color(0xFF171E25)
val Ink950 = Color(0xFF0F141A)

val Sand50 = Color(0xFFFDFAF6)
val Sand100 = Color(0xFFFAF0E7)
val Sand200 = Color(0xFFF3E9DF)
val Sand300 = Color(0xFFE8DACD)

val Sky100 = Color(0xFFD9EDF6)
val Sky600 = Color(0xFF166588)

val Crimson100 = Color(0xFFFADEDB)
val Crimson500 = Color(0xFFB3261E) // --urgency-overdue (light)
val Crimson600 = Color(0xFF961D16)
val CrimsonDark = Color(0xFFF08076) // --status-danger-fg (dark)

val Amber500 = Color(0xFFD99A06) // --urgency-soon (light)

// Semantic values with no ramp entry of their own in the CSS.
val BorderDefaultLight = Color(0xFFE2D9D0) // --border-default (light)
val SurfaceSunkenDark = Color(0xFF0A0E13) // --surface-sunken (dark)
val StatusInfoFgDark = Color(0xFF5CB6D8) // --status-info-fg (dark)
// The alerts surface's `normal` tier chip in dark mode. Its light twin is
// `Sky100` (`--status-info-bg` light is `var(--sky-100)`), but the dark
// scope declares a translucent literal instead of aliasing the ramp — the
// same divergence `UrgencySoonDark` below documents.
val StatusInfoBgDark = Color(0x331E7FA8) // --status-info-bg (dark): sky 20%
val BorderDefaultDark = Color(0x1FFFFFFF) // --border-default (dark): white 12%
val BorderSubtleDark = Color(0x12FFFFFF) // --border-subtle (dark): white 7%
val StatusDangerBgDark = Color(0x38B3261E) // --status-danger-bg (dark)
// `--urgency-soon` diverges between scopes (dark is a literal, not a ramp
// alias) — the same "carry the divergent value under its own name" pattern
// `CrimsonDark`/`StatusInfoFgDark` above already use.
val UrgencySoonDark = Color(0xFFF0B429) // --urgency-soon (dark)
// `--urgency-overdue` diverges the same way: light is `var(--crimson-500)`
// (hence `Crimson500` above) but dark is its own literal, not `CrimsonDark`
// (`--status-danger-fg`) — the same divergent-value pattern as
// `UrgencySoonDark`.
val UrgencyOverdueDark = Color(0xFFF08076) // --urgency-overdue (dark)
