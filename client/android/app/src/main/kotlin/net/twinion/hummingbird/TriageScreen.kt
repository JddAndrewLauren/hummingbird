package net.twinion.hummingbird

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
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
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.ui.LocalWideWindow
import net.twinion.hummingbird.ui.adaptiveGridCells
import net.twinion.hummingbird.ui.contentMaxWidth
import net.twinion.hummingbird.ui.theme.LocalHbDark

// The Triage screen (#531, reshaped by the Triage-parity slice): one queue
// holding both captured and Grilling items in the core's own order, headed
// by the two record-field counts (never recomputed here), rendered through
// the SAME compact card the Now screen's frontier uses (`NowRow.kt` — the
// operator request: same pills, same expansion shape). The selected item
// expands **in its own slot** — the queue loop renders the pane where that
// row was and every other record as its row, the Now screen's in-place
// expansion (#659; `README`'s "In place, not at the top") — and the expanded
// pane IS `ItemDetailPanel`, in `ItemDetailPanelMode.PROMOTE`.
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
    val wide = LocalWideWindow.current
    val listState = rememberLazyGridState()
    val board = (state as? TriageState.Loaded)?.board

    // The opened pane's own ViewModel, by the panel's own key — the SAME
    // instance `ItemDetailPanel` resolves, looked up here because the Back
    // guard below needs its dirtiness while the pane may not be composed at
    // all (`NowScreen`'s own lookup, verbatim).
    val panelViewModel: ItemDetailViewModel? = selectedId?.let { id ->
        viewModel(factory = ItemDetailViewModel.factory(context), key = "item-$id")
    }

    // Where the open pane last sat in the grid — **best-effort, and only a
    // fallback.** It used to be index 0 and needed no remembering; now it is
    // the selected row's own slot, whose index is wherever the core ranked
    // the item. Captured from the layout rather than recomputed from the
    // board, so there is no second copy of the emission order to drift.
    // Keyed on the selection, because a remembered index outlives nothing
    // else: the index a *previous* selection was seen at names an unrelated
    // row for this one. Back re-reads the live layout first and reaches for
    // this only when the pane is currently off screen (`visibleItemsInfo`
    // holds the viewport, not the grid).
    var lastSeenPanePosition by remember(selectedId) { mutableStateOf<Int?>(null) }
    LaunchedEffect(listState, selectedId) {
        val key = selectedId?.let { selectedItemKey(it) } ?: return@LaunchedEffect
        snapshotFlow {
            listState.layoutInfo.visibleItemsInfo.firstOrNull { it.key == key }?.index
        }.collect { index -> if (index != null) lastSeenPanePosition = index }
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
    // Registered at the screen, not inside the pane's grid item: an item
    // scrolled out of the viewport is DISPOSED, taking any handler it
    // registered with it. So while the pane is on screen its own deeper
    // handler wins and the discard confirmation comes first; scrolled away,
    // this one scrolls it back into view where that handler and its dialog
    // take over, rather than silently closing an edit mid-flight
    // (`NowScreen`'s guard, same shape and same reason). An idle Back
    // closes the pane, and with nothing open it pops the entry the way it
    // always did.
    //
    // The scroll branch is taken ONLY while the pane is really in the grid
    // — the board still carries the item — and falls through to closing
    // otherwise (`NowScreen`'s `selectedPaneIsEmitted`, `RecallOverlay`'s
    // shape before it). Since the pane became the selected row's own slot
    // it can be gone with the selection still set (a sync-driven reload
    // that dropped the item, #660), and `reseedIfClean` keeps a dirty draft
    // dirty forever, so without the guard every Back press scrolls to an
    // index that is no longer the pane and does nothing at all: no dialog,
    // no close, no way out. Closing there does NOT discard the typed words:
    // the panel's ViewModel is keyed on the item and outlives the slot, so
    // re-opening shows the draft still dirty and still guarded.
    BackHandler(enabled = selectedId != null) {
        val paneIndex = selectedId
            ?.takeIf { id -> board?.items?.any { it.id == id } == true }
            ?.let { id ->
                val key = selectedItemKey(id)
                listState.layoutInfo.visibleItemsInfo.firstOrNull { it.key == key }?.index
                    ?: lastSeenPanePosition
            }
        if (paneIndex != null && panelViewModel?.isDirty == true) {
            scope.launch { listState.animateScrollToItem(paneIndex) }
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
                    // Uncapped on a wide window: the grid's columns are what
                    // the 880dp cap would otherwise fold back to one.
                    .contentMaxWidth(capped = !wide)
                    // Top 12dp, not the outer 24dp: with the title gone the
                    // counts sit directly under the app row.
                    .padding(start = 24.dp, top = 12.dp, end = 24.dp, bottom = 24.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                // No screen title: the bottom bar already names this tab. The
                // counts keep the header's place as the queue's first line — the
                // record's own fields, never a `board.items.size` recomputation
                // (`capturedCount`/`grillingCount` came decided across the seam).
                if (board != null) {
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

                // **Nothing scrolls on a selection.** The pane opens in the
                // slot of the row that was tapped, so it is already under the
                // reader's finger; the `animateScrollToItem(0)` that used to
                // run here existed only because the pane was somewhere else
                // entirely (index 0), and it was the jump itself that made
                // the first tap and the second tap look like different
                // gestures (README's "In place, not at the top").
                //
                // One scrollable for the whole queue, the Now screen's shape:
                // the opened item is rendered INSIDE the queue loop, in the
                // tapped row's own slot, and the queue keeps rendering around
                // it — never an early return of the editor instead of the
                // list. A grid rather than a list since the unfolded slice:
                // `adaptiveGridCells` is one fixed column on the phone —
                // today's list exactly — and adaptive columns on a wide
                // window, with the pane and the queue's non-row entries
                // spanning every lane.
                LazyVerticalGrid(
                    columns = adaptiveGridCells(),
                    state = listState,
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    // The last row scrolls clear of the Capture FAB.
                    contentPadding = PaddingValues(bottom = 64.dp),
                ) {
                    val current = state

                    when {
                        current is TriageState.Loading -> item(
                            key = "loading",
                            span = { GridItemSpan(maxLineSpan) },
                        ) {
                            CircularProgressIndicator()
                        }
                        board != null && board.items.isEmpty() -> item(
                            key = "empty",
                            span = { GridItemSpan(maxLineSpan) },
                        ) {
                            Text(
                                "Nothing captured is waiting to be sorted.",
                                style = MaterialTheme.typography.bodyLarge,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        board != null -> for (item in board.items) {
                            if (item.id == selectedId) {
                                // **In the row's own place.** The row is not
                                // drawn as well: the pane's header is the
                                // title and its action row carries the row's
                                // mark-done check, so the queue keeps one
                                // line per item.
                                //
                                // **The key names the item.** It was constant
                                // once, on the reasoning that the panel keys
                                // its own state on the item id — which was
                                // wrong twice over, and shipped the trap
                                // `README`'s "The title-edit trap" records: a
                                // constant slot key means the panel is
                                // disposed and recomposed at the SAME slot on
                                // a selection change, and the grid's
                                // `SaveableStateHolder` hands the next item
                                // whatever the last one saved there. Naming
                                // the item is the churn we want: item B's
                                // pane starts as item B's. It is the pane's
                                // key, not the row's, so `listState` can find
                                // it (the dirty-Back handler above).
                                item(
                                    key = selectedItemKey(item.id),
                                    // Full width whatever the column count:
                                    // the pane is the queue's one expanded
                                    // editor, not a card among cards.
                                    span = { GridItemSpan(maxLineSpan) },
                                ) {
                                    Card(
                                        modifier = Modifier.fillMaxWidth(),
                                        colors = CardDefaults.cardColors(
                                            containerColor = MaterialTheme.colorScheme.surface,
                                        ),
                                    ) {
                                        ItemDetailPanel(
                                            itemId = item.id,
                                            syncTick = syncTick,
                                            closeLabel = "Close",
                                            // The panel routes every leaving
                                            // gesture — its ×, its header
                                            // tap, Back — through its own
                                            // dirty-draft confirmation, so
                                            // this only ever fires on a draft
                                            // with nothing to lose.
                                            onClose = { viewModel.closeSelection() },
                                            onGrill = onGrill,
                                            onMutated = { scope.launch { reload() } },
                                            // A promote (or a mark-done)
                                            // takes the item out of this
                                            // queue, so the selection must
                                            // close with it or it dangles at
                                            // a vanished row.
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
                            } else {
                                // The re-tap-to-toggle-shut gesture the row
                                // used to carry is gone with the row: the
                                // open item is never drawn as a row, so
                                // `select(sameId)` is unreachable from here.
                                // The gesture survives in place — the pane's
                                // own header row is a close target sitting
                                // exactly where the row was
                                // (`ItemDetailPanel`'s header), and it routes
                                // through the panel's dirty-draft
                                // confirmation, which is what the guard that
                                // stood here had to hand-roll. A tap on a
                                // different row keeps today's replace
                                // semantics: that row's draft stays in its
                                // own ViewModel.
                                item(key = item.id) {
                                    NowRow(
                                        record = item.asRowModel(),
                                        dark = dark,
                                        selected = false,
                                        onOpen = { viewModel.select(item.id) },
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
    }
}

/** The grid key the open pane takes. One function because two things need
 * to agree on it: the slot that emits the pane (in the selected row's own
 * place) and the screen's dirty-Back handler, which finds the pane's index
 * by this key so it can scroll a disposed panel back into view. Private
 * rather than shared with `NowScreen`'s: the two are separate lists, and
 * neither ever looks a key up in the other's state. */
private fun selectedItemKey(itemId: String) = "selected-item-$itemId"
