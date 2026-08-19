package net.twinion.hummingbird

import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
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
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswerState
import uniffi.hummingbird_ffi_mobile.MobilePaneBand
import uniffi.hummingbird_ffi_mobile.MobileRankedPane

// The ranked-region shell (#536/M4, #537/M4, ADR-0017): the Status screen's
// own `PaneRow`/`RankedPaneList`, pulled out so the Now screen's own three
// panes (#537) render through the identical shell rather than a second
// implementation — the issue's own "through the same pane shell the Status
// screen uses" line. `paneLabel` alone stays a caller-supplied argument:
// which words name a pane's row is a per-surface rendering choice (Status's
// four questions vs. Now's four), never a decision this shell makes.
//
// **This file decides nothing about a pane.** `answerState` and `band`
// arrive already decided ([MobileRankedPane]'s own doc); everything below
// only renders them.

/** [MobilePaneBand]'s dot colour — design-mirror tokens
 * (`.claude/skills/hummingbird-design/tokens/colors.css`), light/dark split
 * exactly [NowScreen.kt]'s own `urgencyColor` mapping notes state the
 * pattern for. Exhaustive, no `else` arm: the compile-time drift gate a
 * `uniffi::Enum` crossing gets everywhere else in this app. */
internal fun bandColor(band: MobilePaneBand, dark: Boolean): Color = when (band) {
    MobilePaneBand.LIVE -> if (dark) UrgencyOverdueDark else Crimson500
    MobilePaneBand.IMMINENT -> if (dark) Ember400 else Ember500
    MobilePaneBand.NEAR -> if (dark) UrgencySoonDark else Amber500
    MobilePaneBand.DISTANT -> if (dark) Ink300 else Ink400
    MobilePaneBand.DORMANT -> if (dark) StatusDoneFgDark else Moss600
}

/** The state sentence beside a pane's dot — decided facts
 * ([MobilePaneAnswerState]/[MobilePaneBand]) in the product's own honest
 * register. Exhaustive, no `else` arm, same discipline as [bandColor]. */
internal fun paneStatusWords(pane: MobileRankedPane): String {
    when (pane.answer.answerState) {
        MobilePaneAnswerState.UNBOUND -> return "Not set up yet"
        MobilePaneAnswerState.BOUND_BUT_UNACQUIRED -> return "Not read yet"
        MobilePaneAnswerState.ANSWERED -> Unit
    }
    return when (pane.answer.band) {
        MobilePaneBand.LIVE -> "Needs attention now"
        MobilePaneBand.IMMINENT -> "Needs attention soon"
        MobilePaneBand.NEAR -> "Worth a look"
        MobilePaneBand.DISTANT -> "Unread"
        MobilePaneBand.DORMANT -> "All quiet"
    }
}

@Composable
internal fun BandDot(band: MobilePaneBand) {
    val dark = isSystemInDarkTheme()
    Column(modifier = Modifier.size(10.dp).background(bandColor(band, dark), CircleShape)) {}
}

@Composable
internal fun PaneRow(pane: MobileRankedPane, label: String) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(16.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            BandDot(pane.answer.band)
            Column(modifier = Modifier.fillMaxWidth()) {
                Text(label, style = MaterialTheme.typography.bodyLarge)
                Text(
                    paneStatusWords(pane),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

/** The ranked-region twin: every pane the seam returned, in the order it
 * returned them — already ADR-0015's cross-pane sort, so this file applies
 * no comparator of its own. `paneLabel` is the one thing a caller supplies:
 * Status's four questions and Now's four name themselves differently, and
 * an unrecognised question is the caller's own drift gate to fail on
 * (`StatusScreen.kt`/`NowScreen.kt`'s own exhaustive `when`s). */
@Composable
internal fun RankedPaneList(panes: List<MobileRankedPane>, paneLabel: (MobileRankedPane) -> String) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        for (pane in panes) {
            PaneRow(pane, paneLabel(pane))
        }
    }
}

/** [RankedPaneList]'s items, for a caller (`NowScreen`) whose panes live
 * inside a single outer `LazyColumn` alongside other sections rather than
 * in their own scrollable list — `Status` renders [RankedPaneList] wrapped
 * in its own `LazyColumn` instead, since its panes are the whole screen. */
internal fun LazyListScope.rankedPaneItems(
    panes: List<MobileRankedPane>,
    paneLabel: (MobileRankedPane) -> String,
) {
    items(panes, key = { it.paneKey }) { pane -> PaneRow(pane, paneLabel(pane)) }
}
