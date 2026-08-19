package net.twinion.hummingbird

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
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
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileStandingQuestion

// The Status screen (#536/M4, ADR-0017): the phone's twin of the web's
// second ranked-region surface — the infra four (model-credit balance,
// GitHub workflow health, uptime, device reachability), through the same
// `hummingbird_core::decisions::panes` shell the web reads.
//
// **This file decides nothing about a pane.** `answerState` and `band`
// arrive already decided (`MobileTaskHost.rankPanes`); the [paneLabel] `when`
// below carries no `else ->` arm on purpose — a ninth standing question
// added core-side is a Kotlin compile error here rather than a row that
// silently renders as nothing (this file's own drift gate, `ffi-mobile::
// MobileStandingQuestion`'s own doc). The band/status-words/dot rendering
// this screen used to own directly moved to `PaneShell.kt` (#537), which
// `NowScreen.kt`'s own three panes now share — see that file's own header
// for why `paneLabel` alone stays per-caller.
//
// Replaces the debug `ProofScreen`. Most of its affordances moved to
// Settings in #535; the one that did not — the "Manage device token in
// Settings" link — moves here instead (#536 review), since #535 left
// `Routes.SETTINGS` reachable only through that one incidental door and
// nothing else in the app navigates there until #541.

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
                is StatusState.Loaded -> LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    rankedPaneItems(current.panes, paneLabel = ::paneLabel)
                }
            }

            TextButton(onClick = onGoToSettings) {
                Text("Manage device token in Settings")
            }
        }
    }
}
