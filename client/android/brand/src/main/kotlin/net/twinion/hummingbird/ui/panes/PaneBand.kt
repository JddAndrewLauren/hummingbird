package net.twinion.hummingbird.ui.panes

import androidx.compose.ui.graphics.Color
import net.twinion.hummingbird.ui.theme.Amber500
import net.twinion.hummingbird.ui.theme.Crimson500
import net.twinion.hummingbird.ui.theme.Ember400
import net.twinion.hummingbird.ui.theme.Ember500
import net.twinion.hummingbird.ui.theme.Ink300
import net.twinion.hummingbird.ui.theme.Ink400
import net.twinion.hummingbird.ui.theme.Moss600
import net.twinion.hummingbird.ui.theme.StatusDoneFgDark
import net.twinion.hummingbird.ui.theme.UrgencyOverdueDark
import net.twinion.hummingbird.ui.theme.UrgencySoonDark
import uniffi.hummingbird_ffi_mobile.MobilePaneBand

// The one band → colour mapping every device app draws a pane's dot with.
// Cut from `PaneShell.kt` for ADR-0039 (the watch draws the same dot, always
// in the dark scheme); the `dark` flag is the caller's, read from whatever
// theme the host runs (`LocalHbDark` on the phone), never decided here.

/** [MobilePaneBand]'s dot colour — design-mirror tokens
 * (`.claude/skills/hummingbird-design/tokens/colors.css`), light/dark split
 * exactly [NowRow.kt]'s own `urgencyColor` mapping notes state the
 * pattern for. Exhaustive, no `else` arm: the compile-time drift gate a
 * `uniffi::Enum` crossing gets everywhere else in this app. */
fun bandColor(band: MobilePaneBand, dark: Boolean): Color = when (band) {
    MobilePaneBand.LIVE -> if (dark) UrgencyOverdueDark else Crimson500
    MobilePaneBand.IMMINENT -> if (dark) Ember400 else Ember500
    MobilePaneBand.NEAR -> if (dark) UrgencySoonDark else Amber500
    MobilePaneBand.DISTANT -> if (dark) Ink300 else Ink400
    MobilePaneBand.DORMANT -> if (dark) StatusDoneFgDark else Moss600
}
