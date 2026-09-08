package net.twinion.hummingbird

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.core.NetworkStatus
import net.twinion.hummingbird.ui.contentMaxWidth
import net.twinion.hummingbird.ui.panes.StatusPartition
import net.twinion.hummingbird.ui.panes.SyncStrip
import net.twinion.hummingbird.ui.panes.statusQuietStack
import net.twinion.hummingbird.ui.theme.Amber600
import net.twinion.hummingbird.ui.theme.LocalHbDark
import net.twinion.hummingbird.ui.theme.Moss600
import net.twinion.hummingbird.ui.theme.StatusDoneFgDark
import net.twinion.hummingbird.ui.theme.StatusWarnFgDark
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileStandingQuestion
import uniffi.hummingbird_ffi_mobile.MobileSyncStatusInput
import uniffi.hummingbird_ffi_mobile.MobileSyncStatusTone
import uniffi.hummingbird_ffi_mobile.syncStatusSummary

// The Status screen (#536/M4, ADR-0017): the phone's twin of the web's
// second ranked-region surface — the infra four (model-credit balance,
// GitHub workflow health, uptime, device reachability), through the same
// `hummingbird_core::decisions::panes` shell the web reads.
//
// Drawn as the design handoff's **quiet stack** since #689: every pane that
// is not both answered and dormant gets a card of its own, in the seam's own
// order, and everything else folds into one card of 44dp chips
// (`ui/panes/StatusQuietStack.kt`, which carries that shape's reasoning).
// The split is a `partition {}` over [StatusPartition] — never a sort, so
// the seam's order survives in both halves.
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
// Replaces the debug `ProofScreen`. Its affordances all live in Settings
// now: the standing "Manage device token in Settings" link this screen
// carried from #536 is **gone**, because `Routes.SETTINGS` has had a
// permanent More-sheet entry since #541 and a second standing door bought
// nothing but the anchored footer that held it — which cost the panes 64dp
// of viewport it could not scroll into. The one Settings door left on this
// screen is contextual and conditional: a pane that answered nothing draws
// its own "Open Settings" (`StatusQuietStack`'s `ProblemCard`), which is
// exactly the credential gap worth offering it for, so [onGoToSettings]
// stays a parameter.

/** One pane's label, from its [MobileStandingQuestion] and its subject —
 * a rendering choice, never a decision: which words name "the GitHub pane"
 * is per-client on the same footing `contract.rs`'s header gives every
 * headline. The six `Homework`/`Scps`/`Waste`/`Weekend`/`Vacation`/`Race` arms
 * cannot reach a Status-surface list (`rank_panes(Status, ..)` never emits them,
 * `panes::mod`'s own test); named individually rather than behind a
 * wildcard so a real tenth question still trips this `when`. */
private fun paneLabel(pane: MobileRankedPane): String = when (pane.standingQuestion) {
    MobileStandingQuestion.KIMI -> "Model credit balance"
    MobileStandingQuestion.GITHUB -> "GitHub workflow — ${pane.subjectKey}"
    MobileStandingQuestion.UPTIME -> "Uptime — ${pane.subjectKey}"
    MobileStandingQuestion.REACHABILITY -> "Device reachability"
    MobileStandingQuestion.POLLER -> "Poller freshness — ${pane.subjectKey}"
    MobileStandingQuestion.HOMEWORK,
    MobileStandingQuestion.SCPS,
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
    /** The door a pane opens when it has no answer at all — drawn only
     * inside `StatusQuietStack`'s `ProblemCard`, never as a standing link
     * on this screen (this file's own header on why the footer one went).
     * A device with no token reaches the one screen that can enter one
     * (`SettingsScreen`'s own token card) from the pane that told it so. */
    onGoToSettings: () -> Unit,
    /** The sync strip's two inputs, threaded from `AppRoot` exactly as
     * `SettingsScreen` already takes them — this screen samples no sync
     * state of its own. */
    lastSyncOutcomeKind: String? = null,
    lastSyncAtMs: Long? = null,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val viewModel: StatusViewModel = viewModel(factory = StatusViewModel.factory(context))
    val state by viewModel.state.collectAsState()
    val expandedKey by viewModel.expandedKey.collectAsState()
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
                    .padding(start = 20.dp, top = 10.dp, end = 20.dp, bottom = 24.dp),
                verticalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                // The screen's own caption. The app icon and wordmark are
                // `MainActivity`'s `AppTopBar`, on every top-level route
                // already — drawing them again here would double the brand
                // on one screen.
                Text(
                    "status",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )

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
                            .weight(1f)
                            .verticalScroll(rememberScrollState()),
                        verticalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        CircularProgressIndicator()
                        CoreFooter(apiVersion = null)
                    }
                    // `weight(1f)`, never a bare `LazyColumn`: an unweighted
                    // lazy list in a `Column` measures against the whole
                    // remaining height and the outer `Column` then has none
                    // left to give. Weighted and *filling* — where #689's
                    // version deliberately did not fill, to measure an
                    // anchored Settings link first — the panes get every
                    // pixel between the caption and the screen edge, and the
                    // footer scrolls with them instead of being reserved for.
                    is StatusState.Loaded -> {
                        // `partition`, never a comparator: both halves come out
                        // in the order the seam ranked them, which is the one
                        // order this screen is allowed to show.
                        val (problems, quiet) = current.panes.partition {
                            StatusPartition.isProblem(it.answer)
                        }
                        LazyColumn(
                            modifier = Modifier.weight(1f),
                            verticalArrangement = Arrangement.spacedBy(14.dp),
                            // The last row scrolls clear of the Capture FAB —
                            // the list screens' own convention, now that this
                            // screen scrolls to its bottom edge like them.
                            contentPadding = PaddingValues(bottom = 64.dp),
                        ) {
                            item(key = "sync-strip") {
                                // The in-process values win when this
                                // session has synced; otherwise the device's
                                // own durable history does. Without that
                                // fallback a cold start read "Not yet
                                // synced" directly above a reachability pane
                                // saying "Synced 4m ago" — two readings of
                                // one fact, on one screen, disagreeing.
                                val summary = syncStatusSummary(
                                    MobileSyncStatusInput(
                                        online = NetworkStatus.isOnline(context),
                                        lastSyncOutcomeKind = lastSyncOutcomeKind
                                            ?: current.syncFacts?.latestOutcomeKind,
                                        lastSyncAtMs = lastSyncAtMs
                                            ?: current.syncFacts?.latestInformativeAtMs,
                                        queueDepth = current.queueDepth ?: 0u,
                                        nowMs = current.rankedAtMs,
                                    ),
                                )
                                SyncStrip(summary, syncToneColor(summary.tone))
                            }
                            if (problems.isEmpty() && quiet.isEmpty()) {
                                // A rank that returned nothing says so. An
                                // empty quiet card would read as "all quiet",
                                // which is ADR-0015's forbidden answer.
                                item(key = "no-panes") {
                                    Text(
                                        "no panes ranked",
                                        style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    )
                                }
                            }
                            statusQuietStack(
                                problems = problems,
                                quiet = quiet,
                                paneLabel = ::paneLabel,
                                nowMs = current.rankedAtMs,
                                expandedKey = expandedKey,
                                onToggleChip = { pane ->
                                    scope.launch { viewModel.toggleExpanded(pane) }
                                },
                                onGoToSettings = onGoToSettings,
                            )
                            // The core line is the list's last row now, not
                            // an anchored block below it: with the list
                            // filling there is no boundary left to clip it
                            // at, and nothing under it worth reserving
                            // height for.
                            item(key = "core-footer") {
                                CoreFooter(apiVersion = current.apiVersion)
                            }
                        }
                    }
                }
            }
        }
    }
}

/** The core's own line — the api version it answered, or that it has not
 * answered yet. Drawn as the pane list's last scrolling row (and under the
 * first-load spinner), never anchored: see this file's header. */
@Composable
private fun CoreFooter(apiVersion: UInt?) {
    Text(
        apiVersion?.let { "api v$it · core ready" } ?: "starting core…",
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.fillMaxWidth(),
        textAlign = TextAlign.Center,
    )
}

/** [MobileSyncStatusTone]'s colour — `SettingsScreen`'s own four-arm
 * mapping, which is where this vocabulary is already drawn. */
@Composable
private fun syncToneColor(tone: MobileSyncStatusTone): androidx.compose.ui.graphics.Color {
    val dark = LocalHbDark.current
    return when (tone) {
        MobileSyncStatusTone.NEUTRAL -> MaterialTheme.colorScheme.onSurfaceVariant
        MobileSyncStatusTone.WARN -> if (dark) StatusWarnFgDark else Amber600
        MobileSyncStatusTone.DANGER -> MaterialTheme.colorScheme.error
        MobileSyncStatusTone.SUCCESS -> if (dark) StatusDoneFgDark else Moss600
    }
}
