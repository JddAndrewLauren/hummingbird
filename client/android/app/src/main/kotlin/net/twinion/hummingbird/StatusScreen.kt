package net.twinion.hummingbird

import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
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
import uniffi.hummingbird_ffi_mobile.MobileStandingQuestion

// The Status screen (#536/M4, ADR-0017): the phone's twin of the web's
// second ranked-region surface — the infra four (model-credit balance,
// GitHub workflow health, uptime, device reachability), through the same
// `hummingbird_core::decisions::panes` shell the web reads.
//
// **This file decides nothing about a pane.** `answerState` and `band`
// arrive already decided (`MobileTaskHost.rankPanes`); every `when` below
// ([paneLabel], [bandColor], [paneStatusWords]) carries no `else ->` arm
// on purpose — a ninth standing question or a sixth band added core-side
// is a Kotlin compile error here rather than a row that silently renders
// as nothing (this file's own drift gate, `ffi-mobile::
// MobileStandingQuestion`'s own doc).
//
// Replaces the debug `ProofScreen`. Most of its affordances moved to
// Settings in #535; the one that did not — the "Manage device token in
// Settings" link — moves here instead (#536 review). `Routes.SETTINGS`
// also has a permanent More-sheet entry since #541; this link stays as a
// second, contextual door, the same way `StatusScreen`'s own review
// requested it.

/** One pane's label, from its [MobileStandingQuestion] and its subject —
 * a rendering choice, never a decision: which words name "the GitHub pane"
 * is per-client on the same footing `contract.rs`'s header gives every
 * headline. The four `Waste`/`Weekend`/`Vacation`/`Race` arms cannot reach
 * a Status-surface list (`rank_panes(Status, ..)` never emits them,
 * `panes::mod`'s own test); named individually rather than behind a
 * wildcard so a real ninth question still trips this `when`. */
private fun paneLabel(pane: MobileRankedPane): String = when (pane.standingQuestion) {
    MobileStandingQuestion.KIMI -> "Model credit balance"
    MobileStandingQuestion.GITHUB -> "GitHub workflow — ${pane.subjectKey}"
    MobileStandingQuestion.UPTIME -> "Uptime — ${pane.subjectKey}"
    MobileStandingQuestion.REACHABILITY -> "Device reachability"
    MobileStandingQuestion.WASTE,
    MobileStandingQuestion.WEEKEND,
    MobileStandingQuestion.VACATION,
    MobileStandingQuestion.RACE ->
        error("a Now-surface question reached the Status screen: ${pane.standingQuestion}")
}

/** The state sentence beside a pane's dot — decided facts
 * ([MobilePaneAnswerState]/[MobilePaneBand]) in the product's own honest
 * register, `MainActivity`'s `describe(RunOutcome)` precedent.
 *
 * None of the status four ever answers [MobilePaneAnswerState.UNBOUND]
 * today (`kimi.rs`/`github.rs`/`uptime.rs`/`reachability.rs` — no per-device
 * binding, so a gap is always [MobilePaneAnswerState.BOUND_BUT_UNACQUIRED]);
 * the arm below is named anyway, on the same "never render as nothing"
 * reasoning `paneLabel`'s exhaustive `when` uses, in case a future pane
 * sunk into `Surface::Status` ever has a binding. */
private fun paneStatusWords(pane: MobileRankedPane): String {
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

/** [MobilePaneBand]'s dot colour — design-mirror tokens
 * (`.claude/skills/hummingbird-design/tokens/colors.css`), light/dark
 * split exactly [urgencyColor]'s own pattern in `NowScreen.kt`. [LIVE]
 * reuses the urgency ramp's most severe rung (`--urgency-overdue`) rather
 * than a fresh red: a pane needing attention *now* is the same salience
 * [urgencyColor] already names, not a second vocabulary for it.
 * [DISTANT] is the one band with no urgency-ramp or status-tone analogue
 * (ADR-0015's own five bands are a superset of the three-band urgency
 * scale), so it takes the plain secondary-text ink rather than inventing a
 * sixth accent colour for "not yet looked at". Exhaustive, no `else` arm —
 * the same discipline [urgencyColor] uses, for the same reason. */
private fun bandColor(band: MobilePaneBand, dark: Boolean): Color = when (band) {
    MobilePaneBand.LIVE -> if (dark) UrgencyOverdueDark else Crimson500
    MobilePaneBand.IMMINENT -> if (dark) Ember400 else Ember500
    MobilePaneBand.NEAR -> if (dark) UrgencySoonDark else Amber500
    MobilePaneBand.DISTANT -> if (dark) Ink300 else Ink400
    MobilePaneBand.DORMANT -> if (dark) StatusDoneFgDark else Moss600
}

@Composable
private fun BandDot(band: MobilePaneBand) {
    val dark = isSystemInDarkTheme()
    Column(modifier = Modifier.size(10.dp).background(bandColor(band, dark), CircleShape)) {}
}

@Composable
private fun PaneRow(pane: MobileRankedPane) {
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
                Text(paneLabel(pane), style = MaterialTheme.typography.bodyLarge)
                Text(
                    paneStatusWords(pane),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

/** The ranked-region twin: every pane [MobileTaskHost.rankPanes] returned,
 * in the order it returned them — already ADR-0015's cross-pane sort, so
 * this file applies no comparator of its own. */
@Composable
private fun RankedPaneList(panes: List<MobileRankedPane>) {
    LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        items(panes, key = { it.paneKey }) { pane -> PaneRow(pane) }
    }
}

@Composable
fun StatusScreen(
    syncTick: Int = 0,
    onBack: () -> Unit,
    /** `ProofScreen`'s one incidental door onto Settings, carried forward
     * (#536 review) — #541 still owns *permanent* nav, but a device with
     * no token needs a way to reach the one screen that can enter one
     * (`SettingsScreen`'s own token card) between now and then, since
     * nothing else in the app currently navigates to
     * [Routes.SETTINGS]. */
    onGoToSettings: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val viewModel: StatusViewModel = viewModel(factory = StatusViewModel.factory(context))
    val state by viewModel.state.collectAsState()

    suspend fun reload() = viewModel.load(System.currentTimeMillis())

    LaunchedEffect(Unit) { reload() }

    LifecycleResumeEffect(Unit) {
        val resumed = scope.launch { reload() }
        onPauseOrDispose { resumed.cancel() }
    }

    LaunchedEffect(syncTick) {
        if (syncTick > 0) reload()
    }

    Scaffold { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text("Status", style = MaterialTheme.typography.headlineLarge)
            TextButton(onClick = onBack) {
                Text("Back")
            }

            when (val current = state) {
                StatusState.Loading -> CircularProgressIndicator()
                is StatusState.Loaded -> RankedPaneList(current.panes)
            }

            TextButton(onClick = onGoToSettings) {
                Text("Manage device token in Settings")
            }
        }
    }
}
