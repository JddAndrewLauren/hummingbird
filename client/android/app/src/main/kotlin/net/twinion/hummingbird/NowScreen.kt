package net.twinion.hummingbird

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.paddingFromBaseline
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalMinimumInteractiveComponentSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.ui.LocalWideWindow
import net.twinion.hummingbird.ui.contentMaxWidth
import net.twinion.hummingbird.ui.panes.NowPaneExpanded
import net.twinion.hummingbird.ui.panes.PaneCollapse
import net.twinion.hummingbird.ui.theme.LocalHbDark
import uniffi.hummingbird_ffi_mobile.MobileFrontierAxis
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileStandingQuestion
import uniffi.hummingbird_ffi_mobile.NowBlockedEntryRecord
import uniffi.hummingbird_ffi_mobile.NowBoardRecord
import uniffi.hummingbird_ffi_mobile.NowColumnRecord
import uniffi.hummingbird_ffi_mobile.NowItemRecord

// M1-6's whole surface (#141/#504), widened to the frontier board by
// M3/#530: the board, decided by `hummingbird-ffi-mobile::MobileTaskHost.
// nowBoard` and rendered verbatim -- this file never orders, groups, bands
// or decides an affordance itself (see `NowViewModel`'s own doc, and
// `lib.rs`'s module header for the Android-never-calls-per-item-decision-
// functions asymmetry with web this screen is the production instance of).
// The phone form is a single vertical grouped list, never a pager: the web
// board's own 390px behaviour -- its wrapping columns stacking into one
// column -- is the reference (`FrontierColumns.tsx`), so this screen is
// that one-column case made native rather than a second design. `MainActivity`'s
// `AppRoot` hosts this composable inside its `NavHost`; the screen carries
// no header links of its own — Alerts and Status are bottom-bar tabs, and
// the M1-era header text links that duplicated them were removed by #588.
// `AppRoot` also owns the foreground sync
// cadence and hands this screen its completion via `syncTick` (see that
// parameter's own note).



/** [MobileFrontierAxis]'s switch label — `AXIS_LABEL` in
 * `FrontierColumns.tsx`, ported. */
private val AXIS_LABEL: Map<MobileFrontierAxis, String> = mapOf(
    MobileFrontierAxis.CONTEXT to "Context",
    MobileFrontierAxis.PROJECT to "Project",
    MobileFrontierAxis.SIZE to "Size",
    MobileFrontierAxis.ENERGY to "Energy",
)

/** [FRONTIER_GROUP_AXES] (`hummingbird_core::decisions::frontier`),
 * mirrored in the switch's own display order — `context` leads because it
 * is the default, exactly the core constant's own doc. */
private val FRONTIER_AXES: List<MobileFrontierAxis> = listOf(
    MobileFrontierAxis.CONTEXT,
    MobileFrontierAxis.PROJECT,
    MobileFrontierAxis.SIZE,
    MobileFrontierAxis.ENERGY,
)

/** Display text for the column of items naming no value on the live axis
 * — `NO_VALUE_LABEL` in `FrontierColumns.tsx`, ported. */
internal val NO_VALUE_LABEL: Map<MobileFrontierAxis, String> = mapOf(
    MobileFrontierAxis.CONTEXT to "No context",
    MobileFrontierAxis.PROJECT to "No project",
    MobileFrontierAxis.SIZE to "No size",
    MobileFrontierAxis.ENERGY to "No energy",
)

/** `hummingbird_domain::Size`'s closed vocabulary — `ItemDetailPanel.kt`'s
 * own list (`listOf("quick", "normal", "deep")`), mirrored for the facet
 * chips rather than re-declared with different values. */
internal val SIZE_VALUES = listOf("quick", "normal", "deep")

/** `hummingbird_domain::Energy`'s closed vocabulary — `ItemDetailPanel.kt`'s
 * own list, mirrored. */
internal val ENERGY_VALUES = listOf("low", "medium", "high")

/** The urgency facet's closed vocabulary — `client/web/src/decisions/
 * seam.ts:387`'s own list and order (`overdue, now, soon`), ported.
 * `calm` is deliberately absent: it is the default, and "a facet for
 * 'nothing pressing' is a facet for 'everything'", which the unpicked
 * state already means. */
private val URGENCY_VALUES = listOf("overdue", "now", "soon")

/** `blockedReasonLabel` (`client/web/src/screens/blocked-reason.ts`),
 * ported verbatim: the reader of a relation-blocked row and the reader of
 * its web twin see the identical words. */
private fun blockedReasonLabel(titles: List<String>): String = when (titles.size) {
    0 -> "Blocked"
    1 -> "Blocked by: ${titles[0]}"
    2 -> "Blocked by: ${titles[0]} and ${titles[1]}"
    else -> "Blocked by: ${titles.dropLast(1).joinToString(", ")} and ${titles.last()}"
}

/** Cards shown per column before the "N more" affordance — `COLUMN_CAP` in
 * `FrontierColumns.tsx`, ported verbatim. */
internal const val COLUMN_CAP = 6

/** The wide board's one list key: on a wide window the whole lane-packed
 * board (`FrontierLaneBoard`) is a single entry of the screen's one lazy
 * list, so this — not the pane's own key — is what the dirty-Back handler
 * scrolls to for a column-ranked pane there. */
private const val WIDE_BOARD_KEY = "board-wide"

/** Which of a capped column's items the board draws: [COLUMN_CAP]'s own
 * first N, plus the open pane's item wherever it ranks. The pane lives in
 * the selected row's own slot now, so a re-rank that pushes the open item
 * past the cap would otherwise make the pane vanish while the selection
 * stayed set — and a selection with no pane in the list is exactly the
 * state [selectedPaneIsEmitted] exists to keep Back out of.
 *
 * Rank order is the seam's ([NowColumnRecord.items] arrives in display
 * order and this never re-orders it, ADR-0025), and the exception is a
 * pure addition: nothing is dropped to make room, so the column shows
 * `COLUMN_CAP + 1` rows in exactly the case where the selected item ranks
 * past the cap. */
internal fun cappedColumnRows(items: List<NowItemRecord>, selectedId: String?): List<NowItemRecord> {
    val capped = items.take(COLUMN_CAP)
    if (selectedId == null ||
        capped.any { it.id == selectedId } ||
        items.none { it.id == selectedId }
    ) {
        return capped
    }
    val cappedIds = capped.map { it.id }.toSet()
    return items.filter { it.id in cappedIds || it.id == selectedId }
}

/** Whether the open pane is actually emitted into the list. The pane is no
 * longer an unconditional entry at index 0: it is drawn only in the
 * selected row's own slot, so it exists only while the board still carries
 * the item (a facet can drop it) and its column is open (the collapse
 * toggle can shut it). Back's dirty branch turns on this — a pane that is
 * not in the list cannot be scrolled into view, and scrolling to a stale
 * index instead is a Back press that does nothing at all.
 *
 * The column cap is deliberately not consulted: [cappedColumnRows] already
 * guarantees an open column draws the selected item whatever its rank.
 * Blocked entries are always drawn (that section has no collapse of its
 * own, [ColumnHeader]'s null `onToggleCollapsed`). */
internal fun selectedPaneIsEmitted(
    board: NowBoardRecord?,
    collapsed: Set<String>,
    selectedId: String?,
): Boolean {
    if (board == null || selectedId == null) return false
    val inOpenColumn = board.columns.any { column ->
        !collapsed.contains(column.value ?: "") && column.items.any { it.id == selectedId }
    }
    return inOpenColumn || board.blocked.any { it.item.id == selectedId }
}

/** One Now-surface pane's label, from its [MobileStandingQuestion] —
 * `StatusScreen.kt`'s own `paneLabel`, this surface's twin: a rendering
 * choice, never a decision. The Status four's arms cannot reach a
 * Now-surface list (`rank_panes(Now, ..)` never emits them, `panes::mod`'s
 * own test); named individually rather than behind a wildcard so a real
 * ninth question still trips this `when`. */
private fun nowPaneLabel(pane: MobileRankedPane): String = when (pane.standingQuestion) {
    MobileStandingQuestion.HOMEWORK -> "What's my homework"
    MobileStandingQuestion.WASTE -> "Bin collection"
    MobileStandingQuestion.WEEKEND -> "This weekend"
    MobileStandingQuestion.VACATION -> "Next trip"
    MobileStandingQuestion.RACE -> "Next race — ${pane.subjectKey}"
    MobileStandingQuestion.KIMI,
    MobileStandingQuestion.GITHUB,
    MobileStandingQuestion.UPTIME,
    MobileStandingQuestion.REACHABILITY ->
        error("a Status-surface question reached the Now screen: ${pane.standingQuestion}")
}

/** Now's own standing-question panes (#537), below the queue — through the
 * same [PaneRow] shell `StatusScreen.kt` renders its own four through
 * (`PaneShell.kt`'s [rankedPaneItems]). Adds nothing while [panes] is empty
 * (the pre-first-load state, [NowViewModel.panes]'s own doc) rather than an
 * empty-state card: unlike the queue, "no panes yet" is never a fact worth
 * reporting on its own, only a moment before the first crossing lands.
 *
 * Appends into the caller's own [LazyListScope] rather than a nested
 * `Column`/`LazyColumn` of its own — the queue and the panes must share one
 * outer scroll, or a frontier taller than the viewport pushes the panes
 * section past the bottom of the screen with nothing to scroll it into view
 * (#537 review). */
private fun LazyListScope.nowPaneSection(
    panes: List<MobileRankedPane>,
    nowMs: Long,
    collapsed: (MobileRankedPane) -> Boolean,
    onToggle: (MobileRankedPane) -> Unit,
    onGoToSettings: () -> Unit,
    /** The weekend card's plan chips (#621) — one do-date write per tap,
     * through [NowViewModel.setScheduledDate], which reloads the panes so
     * the chip fills before any network is touched. */
    onSetScheduledDate: (itemId: String, date: String?) -> Unit,
) {
    if (panes.isEmpty()) return
    item(key = "panes-header") {
        Text("This week", style = MaterialTheme.typography.titleMedium)
    }
    rankedPaneItems(panes,
        paneLabel = ::nowPaneLabel,
        nowMs = nowMs,
        collapsed = collapsed,
        onToggle = onToggle,
        onGoToSettings = onGoToSettings,
        // The Now surface's expanded renderings (the pane-content slice) —
        // dispatched here and nowhere else.
        expandedContent = { pane -> NowPaneExpanded(pane, nowMs, onSetScheduledDate) },
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NowScreen(
    syncTick: Int = 0,
    isRefreshing: Boolean = false,
    onRefresh: () -> Unit = {},
    onGrill: (String) -> Unit = {},
    /** The unbound panes' setup door — Settings through the More stack
     * (`goToTab`), `StatusScreen`'s own second, contextual door shape. */
    onGoToSettings: () -> Unit = {},
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    // In a ViewModelStore, not a composition: see NowViewModel.factory.
    val viewModel: NowViewModel = viewModel(factory = NowViewModel.factory(context))
    val board by viewModel.board.collectAsState()
    val axis by viewModel.axis.collectAsState()
    val facets by viewModel.facets.collectAsState()
    val filtersOpen by viewModel.filtersOpen.collectAsState()
    val collapsed by viewModel.collapsed.collectAsState()
    val expanded by viewModel.expanded.collectAsState()
    val loading by viewModel.loading.collectAsState()
    val panes by viewModel.panes.collectAsState()
    val panesNowMs by viewModel.panesNowMs.collectAsState()
    val paneOverrides by viewModel.paneOverrides.collectAsState()
    val selectedId by viewModel.selectedItemId.collectAsState()
    val statusLine by viewModel.statusLine.collectAsState()
    val dark = LocalHbDark.current
    // The provided width answer (`ui/WindowWidth.kt`): whether the board
    // packs lanes and the content cap comes off. Never re-derived here.
    val wide = LocalWideWindow.current
    val listState = rememberLazyListState()

    // The panel's own ViewModel, by the panel's own key — the SAME
    // instance `ItemDetailPanel` resolves, looked up here because the
    // handler below needs its dirtiness while the panel may not be
    // composed at all.
    val panelViewModel: ItemDetailViewModel? = selectedId?.let { id ->
        viewModel(factory = ItemDetailViewModel.factory(context), key = "item-$id")
    }

    // Where the open pane last sat in the list — **best-effort, and only a
    // fallback.** It used to be index 0 and needed no remembering; now it is
    // the selected row's own slot, whose index depends on which column the
    // item ranks into and how much is expanded above it. Captured from the
    // layout rather than recomputed from the board, so there is no second
    // copy of the emission order to drift. Keyed on the selection, because
    // a remembered index outlives nothing else: collapsing a column above a
    // scrolled-away pane shifts every index below it, and the index a
    // *previous* selection was seen at names an unrelated row for this one.
    // Back re-reads the live layout first and reaches for this only when the
    // pane is currently off screen (`visibleItemsInfo` holds the viewport,
    // not the list).
    var lastSeenPanePosition by remember(selectedId) { mutableStateOf<Int?>(null) }
    // Which list entry holds the open pane. On the phone that is the
    // selected row's own slot; on a wide window a column-ranked pane lives
    // INSIDE the one lane-board entry, so [WIDE_BOARD_KEY] is what Back has
    // to scroll to there — a blocked row's pane keeps a slot of its own on
    // both widths (the Blocked section stays a full-width list either way).
    fun paneListKey(id: String): String =
        if (wide && board?.columns?.any { column -> column.items.any { it.id == id } } == true) {
            WIDE_BOARD_KEY
        } else {
            selectedItemKey(id)
        }
    val paneKey = selectedId?.let { paneListKey(it) }
    LaunchedEffect(listState, paneKey) {
        val key = paneKey ?: return@LaunchedEffect
        snapshotFlow {
            listState.layoutInfo.visibleItemsInfo.firstOrNull { it.key == key }?.index
        }.collect { index -> if (index != null) lastSeenPanePosition = index }
    }

    // Collapse before leaving: with a panel open, Back is "close the item",
    // not "exit the app". While the panel is on screen its own deeper
    // BackHandler wins and the discard confirmation comes first — but the
    // panel is a LazyColumn item, and scrolling it out of the viewport
    // DISPOSES it, unregistering that handler. So a dirty draft is
    // re-checked here: Back then scrolls the panel back into view (where
    // its own handler and dialog take over) rather than silently closing
    // an edit mid-flight.
    //
    // That branch is taken ONLY while the pane is really in the list
    // ([selectedPaneIsEmitted]) — `RecallOverlay`'s own shape, which
    // requires its computed panel index and falls through to closing
    // otherwise. Since the pane became the selected row's own slot it can
    // be gone with the selection still set (collapse the column, or pick a
    // facet that excludes the item), and `reseedIfClean` keeps a dirty
    // draft dirty forever, so without the guard every Back press scrolls to
    // an index that is no longer the pane and does nothing at all: no
    // dialog, no close, no way out.
    //
    // Closing there does NOT discard the typed words: the panel's
    // ViewModel is keyed on the item and outlives the pane's slot, so
    // re-opening the item shows the draft again, still dirty and still
    // guarded by the panel's own confirmation. What is lost is only the
    // guard's *placement* — the confirmation happens the next time the pane
    // is opened rather than now, which is the trade for not trapping the
    // reader behind a pane they cannot see.
    BackHandler(enabled = selectedId != null) {
        val paneIndex = selectedId
            ?.takeIf { selectedPaneIsEmitted(board, collapsed, it) }
            ?.let { id ->
                val key = paneListKey(id)
                listState.layoutInfo.visibleItemsInfo.firstOrNull { it.key == key }?.index
                    ?: lastSeenPanePosition
            }
        if (paneIndex != null && panelViewModel?.isDirty == true) {
            scope.launch { listState.animateScrollToItem(paneIndex) }
        } else {
            viewModel.closeItem()
        }
    }

    suspend fun reload() {
        viewModel.refresh(nowDeadlineShaped())
        // #537: the Now surface's own panes reload alongside the queue —
        // one crossing, `MobileSurface.NOW`'s own board sibling — rather
        // than a second, independently-timed refresh cadence.
        viewModel.loadPanes(System.currentTimeMillis())
    }

    // Foreground refresh on every return to this screen — independent of
    // `syncTick` below, so a capture or an act taken elsewhere (or on
    // another device) shows up the moment this screen is looked at again,
    // even before the next sync cycle completes.
    //
    // The FIRST resume is the initial load, not a second one: a separate
    // one-shot launched effect calling `load` alongside this made entry
    // cross the seam twice, racing a default-axis board against the
    // persisted one (#530's "rendering it makes a single crossing"; the
    // structural gate names the shape that must not come back). `load` restores
    // the axis/collapse set and then refreshes, so it is the resume path's
    // own first iteration rather than a rival to it.
    LifecycleResumeEffect(Unit) {
        val resumed = scope.launch {
            if (viewModel.loadedOnce) {
                reload()
            } else {
                viewModel.load(nowDeadlineShaped())
                viewModel.loadPanes(System.currentTimeMillis())
            }
        }
        onPauseOrDispose { resumed.cancel() }
    }

    // `syncTick` is `AppRoot`'s cadence hand-off (#514 review): the
    // foreground `user`/`timer` sync legs live at the content root now, not
    // on this screen, so this is how Now learns a cycle completed — one
    // whether the tick's own cause was this screen being open or `Status`
    // being open — and re-reads `now_board` rather than rendering a stale
    // mirror until its own next resume.
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
                    // Uncapped on a wide window: the lane board is what the
                    // 880dp cap would otherwise squeeze back to two lanes.
                    .contentMaxWidth(capped = !wide)
                    // No screen title: the bottom bar already names this tab,
                    // so the axis strip is the first thing under the app row
                    // (top 12dp, not the outer 24dp, to keep them adjacent).
                    .padding(start = 24.dp, top = 12.dp, end = 24.dp, bottom = 24.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                val currentBoard = board

                // "N of M shown" — the seam's own pre/post-facet counts
                // ([uniffi.hummingbird_ffi_mobile.NowBoardRecord.shownCount]/
                // [.totalCount], never re-derived here), spoken only while a
                // facet actually narrows the board: an unfiltered "12 of 12
                // shown" is reassurance, not information. Read by the facet
                // panel alone since the axis strip went shrink-to-fit
                // ([AxisRow]'s own doc) — so it says nothing while the panel
                // is shut, and a shut panel with an active filter is spoken
                // by the Filter chip's own count instead.
                val shownLine = currentBoard
                    ?.takeIf { facets.count() > 0 }
                    ?.let { "${it.shownCount} of ${it.totalCount} shown" }

                AxisRow(
                    axis = axis,
                    onPick = { next -> scope.launch { viewModel.setAxis(next, nowDeadlineShaped()) } },
                    filtersOpen = filtersOpen,
                    facetCount = facets.count(),
                    onToggleFilters = { viewModel.toggleFiltersOpen() },
                )

                // The row checkmark's failure line ([NowViewModel.statusLine])
                // — absent until a complete actually fails, `TriageScreen`'s
                // own honesty shape: never a toast, the words stay until the
                // next attempt clears them.
                statusLine?.let {
                    Text(
                        it,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.error,
                    )
                }

                // Behind the disclosure — filtering is the occasional gesture,
                // so only the axis switch earns permanent space
                // (`FrontierColumns.tsx`'s own split, ported).
                if (filtersOpen) {
                    Card(
                        modifier = Modifier.fillMaxWidth(),
                        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
                    ) {
                        Column(modifier = Modifier.padding(12.dp)) {
                            FacetFilterRow(
                                facets = facets,
                                // The live vocabulary the axis's own board carries
                                // (`contexts_of` over the pre-facet list, `board.contexts`
                                // — never a hardcoded suggested list, which would offer a
                                // chip for a context nothing on the board has, or omit one
                                // an item actually carries).
                                contexts = currentBoard?.contexts ?: emptyList(),
                                shownLine = shownLine,
                                onToggle = { facet, value ->
                                    scope.launch { viewModel.toggleFacet(facet, value, nowDeadlineShaped()) }
                                },
                                onClear = { scope.launch { viewModel.clearFacets(nowDeadlineShaped()) } },
                            )
                        }
                    }
                }

                // One LazyColumn for the whole rest of the screen — the queue
                // (whichever of its three states applies) and, appended after
                // it, the now-surface panes (#537). A second, non-scrolling
                // container after this one pushed the panes past the bottom of
                // the viewport with nothing to scroll them into view once the
                // frontier was taller than the screen (#537 review); one shared
                // scroll is the fix, not a `weight` modifier on a still-split
                // layout, since the queue's own three states already need to
                // sit inside *some* `LazyListScope` for `item`/`items` below.
                // **Nothing scrolls on a selection.** The pane opens in the
                // slot of the row that was tapped, so it is already under
                // the reader's finger; the `animateScrollToItem(0)` that
                // used to run here existed only because the pane was
                // somewhere else entirely (index 0), and it was the jump
                // itself that made the first tap and the second tap look
                // like different gestures.
                // The wide board's width, measured — the web's ResizeObserver made
                // native. `maxWidth` is this content column's own bound (the 24dp
                // gutters are already inside it); an unbounded first frame answers
                // null, which `laneCountFor` reads as one lane per column.
                BoxWithConstraints {
                    val boardWidthDp =
                        if (constraints.hasBoundedWidth) maxWidth.value.toInt() else null
                    LazyColumn(
                        state = listState,
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                        // The last row scrolls clear of the Capture FAB.
                        contentPadding = PaddingValues(bottom = 64.dp),
                    ) {
                        // The opened item, INSIDE the board, which keeps rendering
                        // around it — never an early return of the panel instead of
                        // the frontier (ADR-0021 decision 7 / #404: the board
                        // vanishing on tap was the bug). The panel lives in its own
                        // file, so acting/editing/steps stay one implementation with
                        // the notification door's full-screen route.
                        // The pane is NOT an entry of its own here: it is
                        // rendered in the selected row's own slot, further
                        // down, so tapping a card expands *that card* rather
                        // than adding a block at the top of the board
                        // (operator decision 2026-08-20). What it replaced was
                        // an `item(key = "selected-item-$id")` at index 0 plus
                        // an `animateScrollToItem(0)` on every selection
                        // change, which read as two different gestures
                        // depending on where the list happened to be scrolled:
                        // the first tap yanked the board to the top and left
                        // the tapped row far below the pane that was supposed
                        // to be about it, while a second tap — with the list
                        // already at 0 — dropped the new pane roughly where
                        // the finger was and looked like the expansion it was
                        // not.
                        when {
                            loading && currentBoard == null -> item(key = "loading") { CircularProgressIndicator() }
                            currentBoard == null ||
                                (currentBoard.columns.isEmpty() && currentBoard.blocked.isEmpty()) ->
                                item(key = "empty") {
                                    Text(
                                        // "Nothing matches what you picked" and
                                        // "nothing is startable" are different facts
                                        // and must not look alike
                                        // (`FrontierColumns.tsx`'s own empty-result
                                        // branch, ADR-0021 decision 5's whole reason
                                        // for never persisting the filter): a
                                        // facet-emptied board says so, rather than
                                        // reporting an empty frontier it is not.
                                        if (facets.count() > 0) {
                                            "Nothing matches what you picked."
                                        } else {
                                            // Honesty over reassurance (README): an
                                            // empty frontier is reported as a fact,
                                            // not apologised for.
                                            "Nothing on the frontier."
                                        },
                                        style = MaterialTheme.typography.bodyLarge,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    )
                                }
                            else -> {
                                // On a wide window the whole lane-packed board is
                                // ONE entry of this list (`NowLaneBoard.kt`, the
                                // web's frontier-lanes port): the lanes inside it
                                // are non-scrolling, so the screen keeps its one
                                // scroll (#537). The Blocked section and the panes
                                // below stay full-width entries on both widths.
                                // The phone branch is today's loop, untouched.
                                if (wide) item(key = WIDE_BOARD_KEY) {
                                    FrontierLaneBoard(
                                        board = currentBoard,
                                        axis = axis,
                                        collapsed = collapsed,
                                        expanded = expanded,
                                        selectedId = selectedId,
                                        dark = dark,
                                        syncTick = syncTick,
                                        boardWidthDp = boardWidthDp,
                                        onToggleCollapsed = { key ->
                                            scope.launch { viewModel.toggleCollapsed(key) }
                                        },
                                        onToggleExpanded = { key -> viewModel.toggleExpanded(key) },
                                        onSelect = { id -> viewModel.selectItem(id) },
                                        onComplete = { id ->
                                            scope.launch {
                                                viewModel.complete(
                                                    id,
                                                    nowDeadlineShaped(),
                                                    System.currentTimeMillis(),
                                                )
                                            }
                                        },
                                        onCloseItem = { viewModel.closeItem() },
                                        onGrill = onGrill,
                                        onMutated = { scope.launch { reload() } },
                                        onSubmitted = {
                                            viewModel.closeItem()
                                            scope.launch { reload() }
                                        },
                                    )
                                } else for (column in currentBoard.columns) {
                                    val key = column.value ?: ""
                                    val isCollapsed = collapsed.contains(key)
                                    val heading = if (column.value == null) {
                                        NO_VALUE_LABEL[axis] ?: "No value"
                                    } else {
                                        column.label ?: "Project ${column.value}"
                                    }

                                    item(key = "header-$key") {
                                        ColumnHeader(
                                            heading = heading,
                                            count = column.items.size,
                                            collapsed = isCollapsed,
                                            onToggleCollapsed = {
                                                scope.launch { viewModel.toggleCollapsed(key) }
                                            },
                                        )
                                    }

                                    if (!isCollapsed) {
                                        val isExpanded = expanded.contains(key)
                                        // The cap, plus its one exception for the
                                        // open pane — decided by
                                        // [cappedColumnRows], which is where that
                                        // rule is stated and tested.
                                        val visible = if (isExpanded) {
                                            column.items
                                        } else {
                                            cappedColumnRows(column.items, selectedId)
                                        }
                                        val hidden = column.items.size - visible.size

                                        for (record in visible) {
                                            if (record.id == selectedId) {
                                                // **In the row's own place.** The
                                                // key is the pane's, not the
                                                // row's, so `listState` can find
                                                // it (the dirty-Back handler
                                                // above) without knowing which
                                                // column it landed in.
                                                item(key = selectedItemKey(record.id)) {
                                                    SelectedItemCard(
                                                        itemId = record.id,
                                                        syncTick = syncTick,
                                                        onClose = { viewModel.closeItem() },
                                                        onGrill = onGrill,
                                                        onMutated = { scope.launch { reload() } },
                                                        onSubmitted = {
                                                            viewModel.closeItem()
                                                            scope.launch { reload() }
                                                        },
                                                    )
                                                }
                                            } else {
                                                item(key = "$key-${record.id}") {
                                                    NowRow(
                                                        record = record.asRowModel(),
                                                        dark = dark,
                                                        selected = false,
                                                        onOpen = { viewModel.selectItem(record.id) },
                                                        onComplete = {
                                                            scope.launch {
                                                                viewModel.complete(
                                                                    record.id,
                                                                    nowDeadlineShaped(),
                                                                    System.currentTimeMillis(),
                                                                )
                                                            }
                                                        },
                                                    )
                                                }
                                            }
                                        }

                                        if (hidden > 0 || (isExpanded && column.items.size > COLUMN_CAP)) {
                                            item(key = "more-$key") {
                                                TextButton(onClick = { viewModel.toggleExpanded(key) }) {
                                                    Text(if (isExpanded) "Show fewer" else "$hidden more")
                                                }
                                            }
                                        }
                                    }
                                }

                                if (currentBoard.blocked.isNotEmpty()) {
                                    item(key = "blocked-header") {
                                        ColumnHeader(
                                            heading = "Blocked",
                                            count = currentBoard.blocked.size,
                                            collapsed = false,
                                            onToggleCollapsed = null,
                                        )
                                    }
                                    for (entry in currentBoard.blocked) {
                                        if (entry.item.id == selectedId) {
                                            item(key = selectedItemKey(entry.item.id)) {
                                                SelectedItemCard(
                                                    itemId = entry.item.id,
                                                    syncTick = syncTick,
                                                    onClose = { viewModel.closeItem() },
                                                    onGrill = onGrill,
                                                    onMutated = { scope.launch { reload() } },
                                                    onSubmitted = {
                                                        viewModel.closeItem()
                                                        scope.launch { reload() }
                                                    },
                                                )
                                            }
                                        } else {
                                            item(key = "blocked-${entry.item.id}") {
                                                BlockedRow(
                                                    entry = entry,
                                                    dark = dark,
                                                    selected = false,
                                                    onOpen = { viewModel.selectItem(entry.item.id) },
                                                    onComplete = {
                                                        scope.launch {
                                                            viewModel.complete(
                                                                entry.item.id,
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

                        // #537: the now-surface panes — waste/weekend/vacation/race
                        // — below the queue, the same placement the web's own aside
                        // stacks into at 390px (this issue's own "the parity
                        // reference, not a simplification" line). Appended whatever
                        // the queue's own state above was (loading, empty,
                        // populated): the panes are a separate crossing
                        // ([NowViewModel.loadPanes]), never gated on the board's.
                        nowPaneSection(
                            panes = panes,
                            nowMs = panesNowMs,
                            collapsed = { pane ->
                                PaneCollapse.resolve(paneOverrides, pane.paneKey, pane.answer)
                            },
                            onToggle = { pane ->
                                scope.launch { viewModel.togglePaneCollapsed(pane) }
                            },
                            onGoToSettings = onGoToSettings,
                            onSetScheduledDate = { itemId, date ->
                                scope.launch { viewModel.setScheduledDate(itemId, date, panesNowMs) }
                            },
                        )
                    }
                }
            }
        }
    }
}

/** The axis switch — every grouping axis, in [FRONTIER_AXES]'s order —
 * plus the facet panel's one piece of permanent chrome: the Filter
 * disclosure chip, carrying the active-facet count in its label
 * (`FrontierColumns.tsx`'s own row, ported). Never decides which axis
 * groups what; picking one only tells [NowViewModel] which already-decided
 * board to ask for next.
 *
 * **One line, shrunk to fit — never a scroll and never a wrap** (operator
 * decision 2026-08-20, superseding the 2026-08-19 `Row` + `horizontalScroll`
 * that superseded the original `FlowRow`). Three constraints follow from
 * that, and all three are load-bearing:
 *
 * - The five chips fit **272dp** — 320dp, the narrowest width this repo
 *   tests at (`ChoiceRowWrappingTest`'s own qualifier), less `NowScreen`'s
 *   24dp gutters. `AxisRowWrappingTest` measures it, and that measurement
 *   is the only thing standing between this row and a *clipped* trailing
 *   chip: a fixed `Row` squeezes whatever runs out of width, and the chip
 *   at the trailing edge is the Filter disclosure — the only door to an
 *   active filter, hidden with no sign it is there. Measured while
 *   building this: five `FilterChip`s want 320dp and cannot fit at any
 *   text size — a `FilterChip` spends 32dp per chip on horizontal chrome,
 *   160dp of the budget for five. Hence [AxisChip], the same treatment on
 *   12dp of chrome, which wants 276dp.
 * - **The width this fits is the device's, not a stress width** (operator
 *   decision 2026-08-20). The Fold's cover display is 443dp — measured on
 *   hardware, 1080px at density 390 — leaving 419dp of content, so the
 *   strip has 143dp of headroom there. It does **not** fit 272dp, the
 *   320dp figure `ChoiceRowWrappingTest` stresses: measured on the device
 *   at that width, the Filter chip's count digit clips to "Filter ·", and
 *   no type size fixes it. **The accepted limit is roughly 336dp of
 *   content**; below that the trailing chip clips, which is the cost of a
 *   strip that neither wraps nor scrolls, taken deliberately rather than
 *   discovered. The same is true of a large enough font scale.
 * - The label is `bodyMedium`, the sans body style — **not** `labelSmall`.
 *   `labelSmall` is the mono meta style, 11sp Space Mono at +0.08em, and
 *   the design system reserves it for values the system computed; an axis
 *   name is a UI label. It is also the widest small style in the scale, and
 *   using it here cost 44dp the strip did not have.
 * - The 48dp minimum touch target is waived here, deliberately, via
 *   [LocalMinimumInteractiveComponentSize]. This is the one place in the
 *   app that waives it, and what makes it defensible is that it waives
 *   *layout* inflation only: the chips still measure 28dp tall, their full
 *   width is hittable, and the platform expands the touch target at the
 *   input layer regardless of this
 *   (`Modifier.minimumInteractiveComponentSize`'s own doc says so).
 * - There is no leading icon on the Filter chip. It had `ic_search` while
 *   the strip scrolled; measured, the icon is what pushed the row 2dp past
 *   the budget, and the chip says "Filter" in words either way.
 * - The "N of M shown" meta line is not here. It rode beside the Filter
 *   chip while the panel was shut and there is no room for it on one line;
 *   the facet panel's own footer carries it, which is where it was already
 *   said when the panel is open.
 */
@Composable
internal fun AxisRow(
    axis: MobileFrontierAxis,
    onPick: (MobileFrontierAxis) -> Unit,
    filtersOpen: Boolean,
    facetCount: Int,
    onToggleFilters: () -> Unit,
) {
    CompositionLocalProvider(LocalMinimumInteractiveComponentSize provides Dp.Unspecified) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            for (candidate in FRONTIER_AXES) {
                AxisChip(
                    selected = axis == candidate,
                    onClick = { onPick(candidate) },
                    label = AXIS_LABEL[candidate] ?: candidate.name,
                    // One of four, so a radio rather than a checkbox — the
                    // `FilterChip` this replaces called every chip a
                    // checkbox, including these.
                    role = Role.RadioButton,
                )
            }
            AxisChip(
                selected = filtersOpen,
                onClick = onToggleFilters,
                // The middle dot is punctuation, not an icon (design
                // README) — the count rides in the label so an active
                // filter stays visible while the panel is shut.
                label = if (facetCount > 0) "Filter · $facetCount" else "Filter",
                role = Role.Checkbox,
            )
        }
    }
}

/** One chip of the strip above: `FilterChip`'s own treatment — a
 * `secondaryContainer` fill when selected, a hairline outline when not —
 * on a twelfth of its horizontal chrome, which is what lets five of them
 * share a line at 272dp. [AxisRow]'s doc has the measurements and the
 * reason a `FilterChip` cannot be used here; this file's `StageBadge`
 * sibling is the precedent for building a pill from a `Surface` rather
 * than reaching for a Material chip.
 *
 * The whole pill is the target, not the text: `clip` before `selectable`
 * so the ripple stays inside the pill, and the label is `maxLines = 1`
 * because the row has no second line to give it. */
@Composable
private fun AxisChip(
    selected: Boolean,
    onClick: () -> Unit,
    label: String,
    role: Role,
) {
    Box(
        modifier = Modifier
            .height(28.dp)
            .clip(CircleShape)
            .then(
                if (selected) {
                    Modifier.background(MaterialTheme.colorScheme.secondaryContainer)
                } else {
                    Modifier.border(1.dp, MaterialTheme.colorScheme.outline, CircleShape)
                },
            )
            .selectable(selected = selected, role = role, onClick = onClick)
            .padding(horizontal = 6.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium,
            maxLines = 1,
            color = if (selected) {
                MaterialTheme.colorScheme.onSecondaryContainer
            } else {
                MaterialTheme.colorScheme.onSurfaceVariant
            },
        )
    }
}

/** The facet filter — one chip group per facet. Size/Energy/Urgency read
 * from the closed vocabulary lists above; Context reads [contexts] —
 * [uniffi.hummingbird_ffi_mobile.NowBoardRecord.contexts], the live
 * vocabulary `frontier::contexts_of` decided over the current (pre-facet)
 * board, never a hardcoded suggested list. Selection lives in
 * [NowViewModel]'s in-memory [FrontierFacetSelection] and is never
 * persisted (that class's own doc has the reason); toggling only ever
 * asks for a fresh, already-decided board. */
@Composable
private fun FacetFilterRow(
    facets: FrontierFacetSelection,
    contexts: List<String>,
    shownLine: String?,
    onToggle: (FrontierFacet, String) -> Unit,
    onClear: () -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        FacetChipGroup("Context", FrontierFacet.CONTEXT, contexts, facets.context, onToggle)
        FacetChipGroup("Size", FrontierFacet.SIZE, SIZE_VALUES, facets.size, onToggle)
        FacetChipGroup("Energy", FrontierFacet.ENERGY, ENERGY_VALUES, facets.energy, onToggle)
        FacetChipGroup("Urgency", FrontierFacet.URGENCY, URGENCY_VALUES, facets.urgency, onToggle)
        if (facets.count() > 0) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    shownLine ?: "",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                TextButton(onClick = onClear) { Text("Clear filters") }
            }
        }
    }
}

@Composable
internal fun FacetChipGroup(
    label: String,
    facet: FrontierFacet,
    values: List<String>,
    selected: Set<String>,
    onToggle: (FrontierFacet, String) -> Unit,
) {
    // `FacetRow` in `FrontierColumns.tsx` returns nothing for an empty
    // vocabulary (`values.length === 0`) — a board with, say, nothing on
    // it yet offers no Context chips at all rather than a label over an
    // empty row.
    if (values.isEmpty()) return

    // `Alignment.Top`, not `CenterVertically` (#588 item 3): Context is
    // the one group whose vocabulary is live and can wrap the chip row to
    // two lines, and a centred label then floats between them. Top-aligned
    // with a fixed offset, the label sits beside the first line whatever
    // the row's height — the offset centres it against one 32dp chip.
    Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(
            label,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.paddingFromBaseline(top = 20.dp),
        )
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            for (value in values) {
                FilterChip(
                    selected = selected.contains(value),
                    onClick = { onToggle(facet, value) },
                    label = { Text(value) },
                )
            }
        }
    }
}

/** The list key the open pane takes, wherever in the board it lands. One
 * function because two things need to agree on it: the slot that emits the
 * pane (in the selected row's own place) and the screen's dirty-Back
 * handler, which finds the pane's index by this key so it can scroll a
 * disposed panel back into view. */
private fun selectedItemKey(itemId: String) = "selected-item-$itemId"

/** The selected row, expanded in place: the same card shape the rows use,
 * carrying the shared `ItemDetailPanel`.
 *
 * The row it replaces is not drawn as well — the pane's own header is the
 * title, and its action row carries the row's mark-done check — so the
 * board keeps exactly one line per item and the expansion reads as the row
 * growing rather than as a second thing about it appearing elsewhere.
 *
 * [onSubmitted] is separate from [onMutated] for the reason `TriageScreen`
 * states at its own call: a write that lands can take the item off this
 * board (a mark-done does), and a selection left set at a vanished row has
 * no pane — which is also how Back ends up with nothing to scroll to. So
 * the host closes the selection on a landed submit, then reloads. */
@Composable
internal fun SelectedItemCard(
    itemId: String,
    syncTick: Int,
    onClose: () -> Unit,
    onGrill: (String) -> Unit,
    onMutated: () -> Unit,
    onSubmitted: () -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surface,
        ),
    ) {
        ItemDetailPanel(
            itemId = itemId,
            syncTick = syncTick,
            closeLabel = "Close",
            onClose = onClose,
            onGrill = onGrill,
            onMutated = onMutated,
            onSubmitted = onSubmitted,
            modifier = Modifier
                .fillMaxWidth()
                .padding(12.dp),
        )
    }
}

/** One column's (or the Blocked section's) header: its heading, its own
 * count (visible even while collapsed — a closed column must still say
 * how much is inside it), and the collapse toggle. `onToggleCollapsed`
 * is `null` for a section with no collapse of its own (Blocked).
 *
 * The whole row is the toggle, so it carries the design system's 44dp
 * minimum touch target (README: "a 44px row height that doubles as the
 * minimum touch target on every surface") — a `clickable` modifier, unlike
 * Material3's own control components, enforces no minimum of its own, and
 * a `titleMedium` line alone is well under it.
 *
 * The state is drawn with [R.drawable.ic_chevron_down], rotated a
 * quarter-turn when the column is shut, not with a Unicode triangle: the
 * design system's ICONOGRAPHY rule is "Unicode as icons: never". A section
 * with no toggle draws no mark at all rather than a disabled-looking one. */
@Composable
internal fun ColumnHeader(
    heading: String,
    count: Int,
    collapsed: Boolean,
    onToggleCollapsed: (() -> Unit)?,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(
                if (onToggleCollapsed != null) {
                    Modifier.heightIn(min = 44.dp).clickable(onClick = onToggleCollapsed)
                } else {
                    Modifier
                },
            ),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        if (onToggleCollapsed != null) {
            Icon(
                painterResource(R.drawable.ic_chevron_down),
                // The heading says which column; this mark says only
                // whether it is open, so that is what it names.
                contentDescription = if (collapsed) "Expand" else "Collapse",
                modifier = Modifier
                    .size(20.dp)
                    .rotate(if (collapsed) -90f else 0f),
            )
        }
        Text(
            heading,
            style = MaterialTheme.typography.titleMedium,
            modifier = Modifier.weight(1f).padding(start = if (onToggleCollapsed != null) 4.dp else 0.dp),
        )
        Text(
            "$count",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** A relation-blocked row: [NowRow] verbatim, dimmed, with its blocked
 * reason underneath — `NowScreen.tsx`'s own wrapper (opacity 0.6, never a
 * second dimming source stacked on `pending`'s own chip). */
@Composable
private fun BlockedRow(
    entry: NowBlockedEntryRecord,
    dark: Boolean,
    selected: Boolean,
    onOpen: () -> Unit,
    onComplete: () -> Unit,
) {
    Column(
        modifier = Modifier.alpha(0.6f),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        NowRow(
            record = entry.item.asRowModel(),
            dark = dark,
            selected = selected,
            onOpen = onOpen,
            onComplete = onComplete,
        )
        Text(
            blockedReasonLabel(entry.blockedByTitles),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.error,
            modifier = Modifier.padding(start = 12.dp, end = 12.dp),
        )
    }
}
