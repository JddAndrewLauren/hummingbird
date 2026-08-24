package net.twinion.hummingbird

import androidx.compose.ui.graphics.Color
import net.twinion.hummingbird.ui.theme.Amber600
import net.twinion.hummingbird.ui.theme.Crimson600
import net.twinion.hummingbird.ui.theme.CrimsonDark
import net.twinion.hummingbird.ui.theme.StatusWarnFgDark
import uniffi.hummingbird_ffi_mobile.MobilePaneBand

/** The Status nav destination's tint, and the one thing this side decides
 * about it — the Android half of the web's `shell/nav-alarm.ts`.
 *
 * **Which band came back is the core's answer**
 * (`decisions/panes/alarm.rs`, reached through `MobileTaskHost.statusAlarm`),
 * not this file's: whether a gap counts and whether `dormant` counts are the
 * same questions on the phone, the tablet and both web nav forms, so they
 * are answered once, in Rust. What is left here is the last step ADR-0025
 * leaves per-client: which colour a band paints as.
 *
 * The band→tone step is the board's own, ported rather than re-decided —
 * `tile-copy.ts`'s `bandTone`, which the status tiles already apply to these
 * same answers: live/imminent read as danger, near/distant as warn. Keeping
 * the two identical is what stops the button and the tiles it opens onto
 * disagreeing about how bad one answer is.
 *
 * **Plain, not `@Composable`, and `dark` is a parameter.** `MaterialTheme
 * .colorScheme.error` already carries `--status-danger-fg` in both scopes
 * (`Theme.kt`) and reading it would have been the shorter route, at the
 * price of making this whole mapping renderable-only — a Robolectric
 * harness to assert a colour. Taking the two token pairs by name instead
 * leaves the mapping a pure function with a JVM test, and the pairs are the
 * ones `syncStatusToneColor` already reads for the same two tones.
 *
 * `null` in, `null` out: "nothing raises the nav" and "this band is quiet"
 * are one instruction to a caller, and splitting them would make every call
 * site handle two spellings of one answer. */
internal fun navAlarmColor(band: MobilePaneBand?, dark: Boolean): Color? = when (band) {
    null -> null
    // `--status-danger-fg`: `var(--crimson-600)` light, its own literal dark.
    MobilePaneBand.LIVE, MobilePaneBand.IMMINENT -> if (dark) CrimsonDark else Crimson600
    // `--status-warn-fg`: `var(--amber-600)` light, its own literal dark —
    // NOT `UrgencySoonDark`, a different token that happens to share the
    // dark value and would drift the day either moves.
    MobilePaneBand.NEAR, MobilePaneBand.DISTANT -> if (dark) StatusWarnFgDark else Amber600
    // The core folds `dormant` away before it ever reaches here; a nav that
    // invented a third tint for it would be painting "everything is fine"
    // as a state worth noticing.
    MobilePaneBand.DORMANT -> null
}
