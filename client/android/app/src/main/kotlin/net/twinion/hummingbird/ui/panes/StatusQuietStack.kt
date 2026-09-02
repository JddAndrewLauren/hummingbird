package net.twinion.hummingbird.ui.panes

import android.provider.Settings
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import net.twinion.hummingbird.R
import net.twinion.hummingbird.bandColor
import net.twinion.hummingbird.ui.theme.AccentQuietBorderDark
import net.twinion.hummingbird.ui.theme.Ember200
import net.twinion.hummingbird.ui.theme.LocalHbDark
import net.twinion.hummingbird.ui.theme.Moss600
import net.twinion.hummingbird.ui.theme.StatusDoneFgDark
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswer
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswerState
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileStandingQuestion
import uniffi.hummingbird_ffi_mobile.MobileSyncStatusSummary

// The Status screen's **quiet stack** (the design handoff's Android
// direction): every pane that is not both answered and dormant announces
// itself as a card of its own, and everything else folds into one card of
// 44dp chips whose detail opens in place.
//
// This is ADR-0017 decision 1 executed rather than amended — the operator's
// own words for what a status surface should read like are "all green is one
// quiet stack, red announces itself". (The web client's tile board *is* an
// amendment, and ADR-0033 carries it. The two clients diverging here is
// ADR-0025's carve-out working: one set of decided panes, two renderings.)
//
// **This file decides nothing about a pane.** The split is
// [StatusPartition], which reads `band`/`answerState`; the words are all
// `PaneAnswers.kt`'s and `StatusPanesExpanded.kt`'s; the order is the
// seam's, preserved because the screen partitions rather than sorts. It is
// Status-local on purpose: `PaneShell.kt` is shared with Now, and a quiet
// stack is not a shape Now wants.

/** `--radius-card` / `--radius-control` (`tokens/radius.css`), as local
 * shapes rather than a `MaterialTheme.shapes` port: ADR-0026 gates colour
 * and type, spacing and radii have no gate (`ui/ContentMax.kt` records the
 * same thing), and porting the shape scale would restyle every Card and
 * Button in the app for one screen's sake. */
private val CardShape = RoundedCornerShape(14.dp)
private val ChipShape = RoundedCornerShape(10.dp)

/** `--accent-quiet-border`: the ramp alias in light, its own translucent
 * literal in dark (`Color.kt`'s own note on [AccentQuietBorderDark]). */
@Composable
private fun accentQuietBorder(): Color =
    if (LocalHbDark.current) AccentQuietBorderDark else Ember200

@Composable
private fun doneColor(): Color = if (LocalHbDark.current) StatusDoneFgDark else Moss600

/** An announcing card's headline colour.
 *
 * **Not simply `bandColor`.** The core bands every gap `Dormant`
 * (`panes/uptime.rs`, `github.rs`, `kimi.rs`, `reachability.rs` all answer
 * `BoundButUnacquired` + `Band::Dormant`), and `bandColor`'s `DORMANT` arm
 * is the *healthy green* — so an un-polled phone would say "No answer yet"
 * in success green inside an alert border, which is the exact reading
 * [StatusPartition] exists to prevent. A pane with no answer has no band
 * worth colouring: it gets the ordinary text colour, and the card's own
 * accent border is what marks it as wanting attention.
 *
 * `PaneRow` never had this problem because it kept band colour in the 10dp
 * dot and drew its headline in `onSurface`. */
@Composable
private fun headlineColor(answer: MobilePaneAnswer): Color =
    if (answer.answerState == MobilePaneAnswerState.ANSWERED) {
        bandColor(answer.band, LocalHbDark.current)
    } else {
        MaterialTheme.colorScheme.onSurface
    }

/** The uptime services' own glyphs, by subject. A map with a default, not a
 * `when` over a `String`: a `when` would need an `else ->` arm, and this
 * screen's drift rule is that no arm is a wildcard. A service this build has
 * never heard of draws the question's own glyph, which is the graceful arm —
 * a duller icon, never a missing chip. */
private val UPTIME_ICONS = mapOf(
    "authority" to R.drawable.ic_server,
    "web" to R.drawable.ic_globe,
    "runner" to R.drawable.ic_cpu,
)

/** Which glyph identifies a pane. Exhaustive over [MobileStandingQuestion]
 * with the six Now arms erroring exactly as `paneLabel` does — an eleventh
 * question is a compile error here, not a blank chip. */
private fun statusPaneIcon(pane: MobileRankedPane): Int = when (pane.standingQuestion) {
    MobileStandingQuestion.KIMI -> R.drawable.ic_circle_dollar_sign
    MobileStandingQuestion.GITHUB -> R.drawable.ic_git_branch
    MobileStandingQuestion.UPTIME -> UPTIME_ICONS[pane.subjectKey] ?: R.drawable.ic_server
    MobileStandingQuestion.REACHABILITY -> R.drawable.ic_smartphone
    // A meta-question over every other source's own freshness — `radio`,
    // the web's own `tile-vocabulary.ts` choice, not `activity` (the
    // web-side fallback glyph this pane must not share).
    MobileStandingQuestion.POLLER -> R.drawable.ic_radio
    MobileStandingQuestion.HOMEWORK,
    MobileStandingQuestion.SCPS,
    MobileStandingQuestion.WASTE,
    MobileStandingQuestion.WEEKEND,
    MobileStandingQuestion.VACATION,
    MobileStandingQuestion.RACE,
    ->
        error("a Now-surface question reached the Status quiet stack: ${pane.standingQuestion}")
}

/** The expand/collapse spec: `--dur-base` on `--ease-flit`, collapsing to a
 * single millisecond when the device's animator scale is off — the
 * platform's own reduced-motion switch, which `prefers-reduced-motion` is
 * the web's name for. `animateContentSize` is interruptible by
 * construction, so a second tap mid-expansion reverses rather than queues. */
@Composable
private fun expandSpec(): FiniteAnimationSpec<IntSize> {
    val context = LocalContext.current
    // Remembered: this is a `ContentResolver` read, and unremembered it ran
    // on every recomposition — every chip tap — for a value that changes
    // only when the operator changes a system setting.
    val scale = remember(context) {
        Settings.Global.getFloat(
            context.contentResolver,
            Settings.Global.ANIMATOR_DURATION_SCALE,
            1f,
        )
    }
    return if (scale == 0f) tween(1) else tween(200, easing = CubicBezierEasing(.2f, .8f, .2f, 1f))
}

/** "Did this sync work", above everything else on the screen. Every word is
 * the core's: `syncStatusSummary`'s label already carries the queue depth
 * (`· 2 queued`, suppressed at zero), so nothing here re-words it. */
@Composable
internal fun SyncStrip(summary: MobileSyncStatusSummary, toneColor: Color) {
    Card(
        shape = CardShape,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                painterResource(R.drawable.ic_refresh_cw),
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                summary.label,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.weight(1f),
            )
            Text(summary.toneWord, style = MaterialTheme.typography.labelSmall, color = toneColor)
        }
    }
}

/** One announcing pane: its headline in the band's own colour, its full
 * name, and its own facts — always open, never a row to tap. */
@Composable
private fun ProblemCard(
    pane: MobileRankedPane,
    label: String,
    nowMs: Long,
    onGoToSettings: () -> Unit,
) {
    Card(
        shape = CardShape,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        border = BorderStroke(1.dp, accentQuietBorder()),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Icon(
                    painterResource(statusPaneIcon(pane)),
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = headlineColor(pane.answer),
                )
                Text(
                    paneHeadline(pane, nowMs),
                    style = MaterialTheme.typography.titleLarge,
                    color = headlineColor(pane.answer),
                    modifier = Modifier.weight(1f),
                )
                Text(
                    "band:${pane.answer.band.name.lowercase()}",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Text(
                label,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            StatusPaneExpanded(pane, nowMs, headline = false)
            // The door for a pane that has no answer. Gated on "not
            // answered" rather than on `UNBOUND`: **no Status question can
            // ever be unbound** — none has a per-device binding
            // (`panes/mod.rs`'s own test), so they answer
            // `BoundButUnacquired` and an `UNBOUND` branch here is a button
            // nobody can reach. A credential gap is exactly when Settings is
            // worth offering.
            if (pane.answer.answerState != MobilePaneAnswerState.ANSWERED) {
                TextButton(onClick = onGoToSettings) { Text("Open Settings") }
            }
        }
    }
}

/** One 44dp chip — the minimum touch target, and the whole of what a quiet
 * pane says until it is asked. */
@Composable
internal fun QuietChip(
    pane: MobileRankedPane,
    label: String,
    selected: Boolean,
    onToggle: () -> Unit,
) {
    Surface(
        shape = ChipShape,
        color = MaterialTheme.colorScheme.surfaceVariant,
        border = BorderStroke(
            if (selected) 2.dp else 1.dp,
            if (selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outlineVariant,
        ),
        modifier = Modifier
            .size(44.dp)
            // The name is the pane's, not the glyph's: an icon-only target
            // with no accessible name is a blank box to TalkBack, the same
            // rule `PaneGlyphMark` states.
            //
            // `selectable`, not `toggleable`: a toggle publishes a
            // ToggleableState, which TalkBack reads as "ticked / not
            // ticked" — wrong for one-of-many, and doubly odd beside
            // `Role.Tab`. Selection is what this row actually models, and
            // the enclosing row is a `selectableGroup()` so position-in-set
            // is announced with it.
            .selectable(
                selected = selected,
                onClick = onToggle,
                role = Role.Tab,
            ),
    ) {
        Column(
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Icon(
                painterResource(statusPaneIcon(pane)),
                contentDescription = label,
                modifier = Modifier.size(20.dp),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/** Everything that is fine, in one card — a count, a row of chips, and the
 * detail of whichever chip is open. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun QuietCard(
    quiet: List<MobileRankedPane>,
    label: (MobileRankedPane) -> String,
    nowMs: Long,
    expandedKey: String?,
    onToggleChip: (MobileRankedPane) -> Unit,
) {
    val open = quiet.firstOrNull { it.paneKey == expandedKey }
    Card(
        shape = CardShape,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Row(
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Icon(
                    painterResource(R.drawable.ic_circle_check),
                    contentDescription = null,
                    modifier = Modifier.size(16.dp),
                    tint = doneColor(),
                )
                Text(
                    "${quiet.size} as expected",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier.weight(1f),
                )
                Text(
                    "all quiet",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.selectableGroup(),
            ) {
                for (pane in quiet) {
                    QuietChip(
                        pane = pane,
                        label = label(pane),
                        selected = pane.paneKey == expandedKey,
                        onToggle = { onToggleChip(pane) },
                    )
                }
            }
            // The animation sits on the detail region, not on the card: a
            // `LazyColumn` disposes an off-screen item, so an
            // `animateContentSize` on the card itself re-ran from its
            // initial measurement every time the card scrolled back into
            // view — the card visibly grew on scroll-back — and it fought
            // the lazy list's own measurement besides.
            AnimatedVisibility(
                visible = open != null,
                enter = expandVertically(expandSpec()),
                exit = shrinkVertically(expandSpec()),
            ) {
                // Held across the exit animation so the detail does not
                // blank before it has finished collapsing.
                val shown = remember(open) { open }
                if (shown != null) {
                    QuietDetail(shown, label, nowMs)
                }
            }
        }
    }
}

/** One open chip's detail, under the divider. */
@Composable
private fun QuietDetail(
    pane: MobileRankedPane,
    label: (MobileRankedPane) -> String,
    nowMs: Long,
) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        HorizontalDivider(
            thickness = 1.dp,
            color = MaterialTheme.colorScheme.outlineVariant,
        )
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
                label(pane),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                paneHeadline(pane, nowMs),
                style = MaterialTheme.typography.titleLarge,
                color = MaterialTheme.colorScheme.onSurface,
            )
            StatusPaneExpanded(pane, nowMs, headline = false)
        }
    }
}

/** The stack itself: announcing panes first, in the seam's own order, then
 * the one quiet card — and no quiet card at all when nothing is quiet,
 * rather than an empty card claiming everything is fine. */
internal fun LazyListScope.statusQuietStack(
    problems: List<MobileRankedPane>,
    quiet: List<MobileRankedPane>,
    paneLabel: (MobileRankedPane) -> String,
    nowMs: Long,
    expandedKey: String?,
    onToggleChip: (MobileRankedPane) -> Unit,
    onGoToSettings: () -> Unit,
) {
    items(problems, key = { it.paneKey }) { pane ->
        ProblemCard(
            pane = pane,
            label = paneLabel(pane),
            nowMs = nowMs,
            onGoToSettings = onGoToSettings,
        )
    }
    if (quiet.isNotEmpty()) {
        item(key = "quiet-card") {
            QuietCard(
                quiet = quiet,
                label = paneLabel,
                nowMs = nowMs,
                expandedKey = expandedKey,
                onToggleChip = onToggleChip,
            )
        }
    }
}
