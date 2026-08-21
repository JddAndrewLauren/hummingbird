package net.twinion.hummingbird

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
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
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.ui.contentMaxWidth
import net.twinion.hummingbird.ui.theme.LocalHbDark

// The Triage screen (#531, reshaped by the Triage-parity slice): one queue
// holding both captured and Grilling items in the core's own order, headed
// by the two record-field counts (never recomputed here), rendered through
// the SAME compact card the Now screen's frontier uses (`NowRow.kt` — the
// operator request: same pills, same expansion shape). The selected item
// expands at index 0 of the one LazyColumn, above the queue, exactly the
// Now screen's inline-expansion pattern — and the expanded pane IS
// `ItemDetailPanel`, in `ItemDetailPanelMode.PROMOTE`.
//
// That mode is what keeps #360: promote-to-Ready is the only submit the
// pane offers here, so the panel's plain `save` — the non-promoting write
// this surface bans — is unreachable from Triage. The two facts that used
// to argue for a separate editor are both answered on the record itself:
// `available_actions` is empty for the Triage and Grilling stages, but
// `can_mark_done` rides beside it and gates the check, and the shared
// `ui/forms` field set is what both surfaces now render. The row checkmark
// still goes through `act` — never a triage.
//
// **The Grill button is live (#539).** It navigates to the standalone
// takeover (`GrillTakeoverScreen.kt`) rather than opening an interview
// inline — this screen holds no turn/draft state of its own, gated on the
// row's own `canGrill`/`hasGrillDraft` facts from the seam.
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TriageScreen(
    syncTick: Int = 0,
    isRefreshing: Boolean = false,
    onRefresh: () -> Unit = {},
    onGrill: (String) -> Unit = {},
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val viewModel: TriageViewModel = viewModel(factory = TriageViewModel.factory(context))
    val state by viewModel.state.collectAsState()
    val statusLine by viewModel.statusLine.collectAsState()
    val selectedId by viewModel.selectedId.collectAsState()
    val dark = LocalHbDark.current
    val listState = rememberLazyListState()

    // The opened pane's own ViewModel, by the panel's own key — the SAME
    // instance `ItemDetailPanel` resolves, looked up here because the Back
    // guard below needs its dirtiness while the pane may not be composed at
    // all (`NowScreen`'s own lookup, verbatim).
    val panelViewModel: ItemDetailViewModel? = selectedId?.let { id ->
        viewModel(factory = ItemDetailViewModel.factory(context), key = "item-$id")
    }

    suspend fun reload() {
        viewModel.load(nowDeadlineShaped())
    }

    LaunchedEffect(Unit) { reload() }

    // Refresh on every return to this screen, independent of the sync
    // cadence (`AlertsScreen`'s own precedent) — a capture minted from
    // `CaptureActivity` while this screen was backgrounded must not wait
    // for the next app-wide tick to appear in the queue.
    LifecycleResumeEffect(Unit) {
        val resumed = scope.launch { reload() }
        onPauseOrDispose { resumed.cancel() }
    }

    // `AppRoot`'s cadence hand-off (#514's shape): one increment per
    // completed sync cycle, so this screen re-reads the mirror after each
    // one rather than showing a stale queue until its own next resume.
    LaunchedEffect(syncTick) {
        if (syncTick > 0) reload()
    }

    // System Back with typed edits open: the words a person wrote are never
    // thrown away silently — the house rule `ItemDetailPanel`'s header
    // states.
    //
    // Registered at the screen, not inside the pane's LazyColumn item: an
    // item scrolled out of the viewport is DISPOSED, taking any handler it
    // registered with it. So while the pane is on screen its own deeper
    // handler wins and the discard confirmation comes first; scrolled away,
    // this one scrolls it back into view where that handler and its dialog
    // take over, rather than silently closing an edit mid-flight
    // (`NowScreen`'s guard, same shape and same reason). An idle Back
    // closes the pane, and with nothing open it pops the entry the way it
    // always did.
    BackHandler(enabled = selectedId != null) {
        if (panelViewModel?.isDirty == true) {
            scope.launch { listState.animateScrollToItem(0) }
        } else {
            viewModel.closeSelection()
        }
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
                    // counts sit directly under the app row.
                    .padding(start = 24.dp, top = 12.dp, end = 24.dp, bottom = 24.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                // No screen title: the bottom bar already names this tab. The
                // counts keep the header's place as the queue's first line — the
                // record's own fields, never a `board.items.size` recomputation
                // (`capturedCount`/`grillingCount` came decided across the seam).
                (state as? TriageState.Loaded)?.board?.let { board ->
                    Text(
                        "${board.capturedCount} captured · ${board.grillingCount} grilling",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }

                statusLine?.let {
                    Text(
                        it,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.error,
                    )
                }

                // NowScreen's own selection scroll: only on a CHANGE of
                // selection — `remember` starts equal to whatever an Activity
                // recreation restored, so a fold/unfold keeps its scroll
                // position instead of animating back to top.
                var lastScrolledSelection by remember { mutableStateOf(selectedId) }
                LaunchedEffect(selectedId) {
                    if (selectedId != null && selectedId != lastScrolledSelection) {
                        listState.animateScrollToItem(0)
                    }
                    lastScrolledSelection = selectedId
                }

                // One LazyColumn for the whole queue, the Now screen's shape:
                // the opened item is always index 0 when present, ABOVE the
                // queue, which keeps rendering below it — never an early
                // return of the editor instead of the list.
                LazyColumn(
                    state = listState,
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                    // The last row scrolls clear of the Capture FAB.
                    contentPadding = PaddingValues(bottom = 64.dp),
                ) {
                    val current = state
                    val board = (current as? TriageState.Loaded)?.board

                    // **The key names the item.** It was constant once, on
                    // the reasoning that the panel keys its own state on the
                    // item id — which was wrong twice over, and shipped the
                    // trap `README`'s "The title-edit trap" records: a
                    // constant slot key means the panel is disposed and
                    // recomposed at the SAME slot on a selection change, and
                    // LazyColumn's `SaveableStateHolder` hands the next item
                    // whatever the last one saved there. Naming the item is
                    // the churn we want: item B's pane starts as item B's.
                    selectedId?.let { id ->
                        if (board?.items?.any { it.id == id } == true) {
                            item(key = "selected-item-$id") {
                                Card(
                                    modifier = Modifier.fillMaxWidth(),
                                    colors = CardDefaults.cardColors(
                                        containerColor = MaterialTheme.colorScheme.surface,
                                    ),
                                ) {
                                    ItemDetailPanel(
                                        itemId = id,
                                        syncTick = syncTick,
                                        closeLabel = "Close",
                                        // The panel routes every leaving
                                        // gesture — its ×, its header tap,
                                        // Back — through its own dirty-draft
                                        // confirmation, so this only ever
                                        // fires on a draft with nothing to
                                        // lose.
                                        onClose = { viewModel.closeSelection() },
                                        onGrill = onGrill,
                                        onMutated = { scope.launch { reload() } },
                                        // A promote (or a mark-done) takes
                                        // the item out of this queue, so the
                                        // selection must close with it or it
                                        // dangles at a vanished row.
                                        onSubmitted = {
                                            viewModel.closeSelection()
                                            scope.launch { reload() }
                                        },
                                        mode = ItemDetailPanelMode.PROMOTE,
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .padding(12.dp),
                                    )
                                }
                            }
                        }
                    }

                    when {
                        current is TriageState.Loading -> item(key = "loading") {
                            CircularProgressIndicator()
                        }
                        board != null && board.items.isEmpty() -> item(key = "empty") {
                            Text(
                                "Nothing captured is waiting to be sorted.",
                                style = MaterialTheme.typography.bodyLarge,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        board != null -> items(board.items, key = { it.id }) { item ->
                            NowRow(
                                record = item.asRowModel(),
                                dark = dark,
                                selected = item.id == selectedId,
                                // Re-tapping the row whose pane is already
                                // open is `select(sameId)`, which the
                                // ViewModel treats as a toggle shut — the
                                // one leaving gesture that does not pass
                                // through the panel, so it asks the panel
                                // whether there is anything to lose and
                                // scrolls its dialog into view if there is.
                                // A tap on a *different* row keeps today's
                                // replace semantics: that row's draft stays
                                // in its own ViewModel.
                                onOpen = {
                                    if (item.id == selectedId && panelViewModel?.isDirty == true) {
                                        scope.launch { listState.animateScrollToItem(0) }
                                    } else {
                                        viewModel.select(item.id)
                                    }
                                },
                                onComplete = {
                                    scope.launch {
                                        viewModel.complete(
                                            item.id,
                                            nowDeadlineShaped(),
                                            System.currentTimeMillis(),
                                        )
                                    }
                                },
                            )
                        }
                    }
                }
            }
        }
    }
}
