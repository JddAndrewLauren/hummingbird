package net.twinion.hummingbird

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
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
import net.twinion.hummingbird.ui.contentMaxWidth
import net.twinion.hummingbird.ui.panes.PaneCollapse
import net.twinion.hummingbird.ui.panes.StatusPaneExpanded
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

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun StatusScreen(
    syncTick: Int = 0,
    isRefreshing: Boolean = false,
    onRefresh: () -> Unit = {},
    /** `ProofScreen`'s one incidental door onto Settings, carried forward
     * (#536 review). [Routes.SETTINGS] has a permanent More-sheet entry
     * since #541; this link stays as a second, contextual door — a device
     * with no token reaches the one screen that can enter one
     * (`SettingsScreen`'s own token card) from the surface that told it
     * so, this file's own header. */
    onGoToSettings: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val viewModel: StatusViewModel = viewModel(factory = StatusViewModel.factory(context))
    val state by viewModel.state.collectAsState()
    val paneOverrides by viewModel.paneOverrides.collectAsState()
    val statusLine by viewModel.statusLine.collectAsState()

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
        // The pull gesture is a second door onto AppRoot's one sync cadence
        // (`sync("user")` via [onRefresh]) — never a screen-local cycle; the
        // reload itself still arrives through `syncTick` when the cycle lands.
        PullToRefreshBox(
            isRefreshing = isRefreshing,
            onRefresh = onRefresh,
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .contentMaxWidth()
                    // Top 12dp, not the outer 24dp: with the title gone the
                    // panes sit directly under the app row.
                    .padding(start = 24.dp, top = 12.dp, end = 24.dp, bottom = 24.dp)
                    // A fixed inset, unlike the list screens' scrolled
                    // clearance: the Settings link below the weighted list is
                    // anchored, not scrolled, so only shrinking the viewport
                    // keeps it clear of the Capture FAB.
                    .padding(bottom = 64.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                // A failed rank, worded — the same line every other screen
                // carries, above whatever the last good read left standing.
                statusLine?.let {
                    Text(
                        it,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.error,
                    )
                }

                when (val current = state) {
                    // A scrollable around the spinner so the pull gesture works
                    // before the first load lands too.
                    StatusState.Loading -> Column(
                        modifier = Modifier
                            .weight(1f, fill = false)
                            .verticalScroll(rememberScrollState()),
                    ) {
                        CircularProgressIndicator()
                    }
                    // `weight(fill = false)`, never a bare `LazyColumn`: an
                    // unweighted lazy list in a `Column` measures against the
                    // whole remaining height, and enough GitHub/uptime panes
                    // then push the Settings link below the viewport with
                    // nothing to scroll it back into view. Weighted, the link
                    // is measured first and the panes take what is left;
                    // `fill = false` keeps a short list from stranding the link
                    // at the bottom of the screen.
                    is StatusState.Loaded -> LazyColumn(
                        modifier = Modifier.weight(1f, fill = false),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        rankedPaneItems(
                            current.panes,
                            paneLabel = ::paneLabel,
                            nowMs = current.rankedAtMs,
                            collapsed = { pane ->
                                PaneCollapse.resolve(paneOverrides, pane.paneKey, pane.answer)
                            },
                            onToggle = { pane ->
                                scope.launch { viewModel.togglePaneCollapsed(pane) }
                            },
                            onGoToSettings = onGoToSettings,
                            // The Status four's expanded renderings (the
                            // pane-content slice) — dispatched here and
                            // nowhere else.
                            expandedContent = { pane ->
                                StatusPaneExpanded(pane, current.rankedAtMs)
                            },
                        )
                    }
                }

                TextButton(onClick = onGoToSettings) {
                    Text("Manage device token in Settings")
                }
            }
        }
    }
}
