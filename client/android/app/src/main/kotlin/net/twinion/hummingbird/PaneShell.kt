package net.twinion.hummingbird

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import net.twinion.hummingbird.ui.panes.MAX_GLYPHS
import net.twinion.hummingbird.ui.panes.PaneGlyph
import net.twinion.hummingbird.ui.panes.paneGlyphs
import net.twinion.hummingbird.ui.panes.paneHeadline
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

// The ranked-region shell (#536/M4, #537/M4, ADR-0017; two-form since the
// pane-parity slice): the web `RankedRegion.tsx`'s own contract, ported —
// every pane has exactly two forms, and the SHELL draws the collapsed one
// for all of them. Collapsed: the band dot, the question's label, up to
// [MAX_GLYPHS] of the pane's own marks, and its one-line headline
// (`ui/panes/PaneAnswers.kt`, the per-question words). Expanded: the same
// header as a collapse control, the headline, the unbound setup door
// ("Open Settings" — a gap is an actionable fact, never a blank pane), and
// whatever expanded content the caller supplies per question (the
// pane-content slices fill that slot). Tap toggles; whether a pane STARTS
// collapsed is `ui/panes/PaneCollapse.kt`'s band-stamped rule, resolved by
// the caller's ViewModel, never here.
//
// **This file decides nothing about a pane.** `answerState` and `band`
// arrive already decided ([MobileRankedPane]'s own doc), the facts arrive
// decided beside them, and the words are `PaneAnswers.kt`'s — everything
// below only renders them. `paneLabel` alone stays a caller-supplied
// argument: which words name a pane's row is a per-surface rendering
// choice (Status's four questions vs. Now's four), never a decision this
// shell makes.

/** [MobilePaneBand]'s dot colour — design-mirror tokens
 * (`.claude/skills/hummingbird-design/tokens/colors.css`), light/dark split
 * exactly [NowRow.kt]'s own `urgencyColor` mapping notes state the
 * pattern for. Exhaustive, no `else` arm: the compile-time drift gate a
 * `uniffi::Enum` crossing gets everywhere else in this app. */
internal fun bandColor(band: MobilePaneBand, dark: Boolean): Color = when (band) {
    MobilePaneBand.LIVE -> if (dark) UrgencyOverdueDark else Crimson500
    MobilePaneBand.IMMINENT -> if (dark) Ember400 else Ember500
    MobilePaneBand.NEAR -> if (dark) UrgencySoonDark else Amber500
    MobilePaneBand.DISTANT -> if (dark) Ink300 else Ink400
    MobilePaneBand.DORMANT -> if (dark) StatusDoneFgDark else Moss600
}

@Composable
internal fun BandDot(band: MobilePaneBand) {
    val dark = isSystemInDarkTheme()
    Column(modifier = Modifier.size(10.dp).background(bandColor(band, dark), CircleShape)) {}
}

/** One collapsed-row mark — the web contract's two arms: an 8dp dot with a
 * 1dp edge (a kerbside bin), or a 13dp icon in the muted text colour
 * (icons never carry colour independently of their label). Either way the
 * [PaneGlyph.label] is the accessible name — a dot with no name is a blank
 * box to TalkBack. */
@Composable
internal fun PaneGlyphMark(glyph: PaneGlyph) {
    when (glyph) {
        is PaneGlyph.Dot -> Column(
            modifier = Modifier
                .size(8.dp)
                .background(glyph.fill, CircleShape)
                .border(1.dp, glyph.edge, CircleShape),
        ) {}
        is PaneGlyph.Icon -> Icon(
            painterResource(glyph.iconRes),
            contentDescription = glyph.label,
            modifier = Modifier.size(13.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
internal fun PaneRow(
    pane: MobileRankedPane,
    label: String,
    nowMs: Long,
    collapsed: Boolean,
    onToggle: () -> Unit,
    onGoToSettings: () -> Unit,
    expandedContent: (@Composable (MobileRankedPane) -> Unit)?,
) {
    val headline = paneHeadline(pane, nowMs)
    // Bounded HERE, never trusted to the pane — the cap protects the row
    // from the pane (the web contract's own `boundedGlyphs`).
    val glyphs = paneGlyphs(pane, nowMs).take(MAX_GLYPHS)

    if (collapsed) {
        Card(
            onClick = onToggle,
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 44.dp)
                    .padding(horizontal = 16.dp, vertical = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                BandDot(pane.answer.band)
                Text(
                    label,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                for (glyph in glyphs) {
                    PaneGlyphMark(glyph)
                }
                Text(
                    headline,
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.weight(1f),
                    textAlign = TextAlign.End,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    } else {
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = 44.dp)
                        .clickable(onClickLabel = "Collapse") { onToggle() },
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    BandDot(pane.answer.band)
                    Text(
                        label,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.weight(1f),
                    )
                    Icon(
                        painterResource(R.drawable.ic_chevron_down),
                        contentDescription = null,
                        modifier = Modifier.size(16.dp),
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Text(headline, style = MaterialTheme.typography.bodyLarge)
                // An unbound question still renders — with its door: the
                // reader can act on "nobody asked this yet" exactly one
                // way, and the web's setup prompt carries the same button.
                if (pane.answer.answerState == MobilePaneAnswerState.UNBOUND) {
                    TextButton(onClick = onGoToSettings) {
                        Text("Open Settings")
                    }
                }
                expandedContent?.invoke(pane)
            }
        }
    }
}

/** The ranked-region twin: every pane the seam returned, in the order it
 * returned them — already ADR-0015's cross-pane sort, so this file applies
 * no comparator of its own. `paneLabel` is the one thing a caller names
 * per surface; `collapsed`/`onToggle` are the ViewModel-resolved
 * band-stamped collapse (`PaneCollapse`), `onGoToSettings` the unbound
 * setup door, and `expandedContent` the per-question expanded rendering
 * (null until the pane-content slices fill it).
 *
 * Both callers append these into a `LazyColumn` they already own rather
 * than a `Column` of their own (#537 review — a plain `Column` placed
 * after, or beside, an unweighted `LazyColumn` can lay out past the
 * viewport with nothing to scroll it into view): `NowScreen` appends it
 * into the same `LazyColumn` the queue's rows already populate, and
 * `StatusScreen` wraps this call in a dedicated weighted `LazyColumn` of
 * its own (that file's own note says why `fill = false`). */
internal fun LazyListScope.rankedPaneItems(
    panes: List<MobileRankedPane>,
    paneLabel: (MobileRankedPane) -> String,
    nowMs: Long,
    collapsed: (MobileRankedPane) -> Boolean,
    onToggle: (MobileRankedPane) -> Unit,
    onGoToSettings: () -> Unit,
    expandedContent: (@Composable (MobileRankedPane) -> Unit)? = null,
) {
    items(panes, key = { it.paneKey }) { pane ->
        PaneRow(
            pane = pane,
            label = paneLabel(pane),
            nowMs = nowMs,
            collapsed = collapsed(pane),
            onToggle = { onToggle(pane) },
            onGoToSettings = onGoToSettings,
            expandedContent = expandedContent,
        )
    }
}
