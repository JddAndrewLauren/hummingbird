package net.twinion.hummingbird

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.ui.contentMaxWidth
import uniffi.hummingbird_ffi_mobile.MobileRecallGroup
import uniffi.hummingbird_ffi_mobile.MobileRecallRowRecord

// Recall (#542, #478/CONTEXT.md, reshaped by the search-overlay slice):
// re-find one known item across everything the mirror has ever known, live
// or archived — the phone's counterpart to `RecallOverlay.tsx`, and since
// this slice the same SHAPE too (operator request 2026-08-20): an overlay
// `AppRoot` draws over whatever screen is showing, not a navigated route.
// Opening it changes no destination; Back (or the X) closes it and the
// screen underneath was there all along, exactly the web's scrim-dialog
// contract. Search-as-you-type, no debounce (`RecallViewModel`'s own doc).
//
// Tapping a live row expands it IN PLACE into the shared `ItemDetailPanel`
// below the row (`RecallOverlay.tsx`'s own expansion, and the same panel
// Now's inline door hosts) — never a navigation; the overlay's LazyColumn
// is the scroll host the panel's "the panel never scrolls itself" contract
// asks for. A Done or archived row is shown, labelled, dimmed, and not
// tappable (#597): the alpha is what says so — an inert row at full
// opacity carries every affordance cue of the live row above it and
// answers none of them.
//
// This file decides nothing about a row or the result set.
// [MobileRecallRowRecord]s arrive already matched, grouped and ordered;
// `total` is the seam's own un-capped count, never re-derived from
// `rows.size`. There is no sort, filter or group-by anywhere below —
// `RecallScreenStructuralTest` gates that.

/** The alpha of a row that answers no tap — Done and archived alike
 * (#597): the dimming tracks inertness, not the archive flag, so the two
 * inert groups read the same and only the tappable one reads solid.
 * `LedgerScreen` declares the same 0.72 for its own archived rows. */
private const val INERT_ALPHA = 0.72f

@Composable
internal fun RecallOverlay(
    /** `AppRoot`'s own instance, passed rather than resolved here: the open
     * gesture there resets the selection on it (`openRecall`), and that is
     * only the right reset if it reaches the instance this overlay reads. */
    viewModel: RecallViewModel,
    syncTick: Int,
    onClose: () -> Unit,
    onGrill: (String) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val query by viewModel.query.collectAsState()
    val rows by viewModel.rows.collectAsState()
    val total by viewModel.total.collectAsState()
    val loading by viewModel.loading.collectAsState()
    val statusLine by viewModel.statusLine.collectAsState()
    val selectedId by viewModel.selectedId.collectAsState()

    LaunchedEffect(query) {
        viewModel.search(System.currentTimeMillis())
    }

    // A fresh open starts with no row expanded — the web resets its
    // `selectedId` whenever the overlay is freshly opened. That reset is
    // `AppRoot.openRecall`'s, at the gesture, NOT a `LaunchedEffect(Unit)`
    // here: this composition re-enters on every Activity recreation (a
    // fold/unfold on the install target), and clearing there would collapse
    // an open, possibly mid-edit, panel that nobody asked to close. The
    // QUERY survives either way (the ViewModel holds it), matching the
    // web's App-owned query state.

    val listState = rememberLazyListState()

    // The panel's own ViewModel, by the panel's own key — the SAME instance
    // `ItemDetailPanel` resolves, looked up here because the handler below
    // needs its dirtiness while the panel may not be composed at all.
    // `NowScreen`'s guard, ported for the identical trap: while the panel is
    // on screen its own deeper BackHandler wins and the discard
    // confirmation comes first, but the panel is a LazyColumn item, and
    // scrolling it out of the viewport DISPOSES it, unregistering that
    // handler — leaving the overlay's own Back to dismiss straight past the
    // confirm gate, taking the words with it. So a dirty draft is
    // re-checked here and Back scrolls the panel back into view instead.
    val panelViewModel: ItemDetailViewModel? = selectedId?.let { id ->
        viewModel(factory = ItemDetailViewModel.factory(context), key = "item-$id")
    }

    // Back closes the overlay, never navigates the screen underneath.
    BackHandler {
        val panelIndex = selectedId
            ?.let { id -> rows.indexOfFirst { it.id == id } }
            ?.takeIf { it >= 0 }
            ?.let { it + 1 } // the detail item sits directly below its row
        if (panelIndex != null && panelViewModel?.draft?.value != null && panelViewModel.isDirty) {
            scope.launch { listState.animateScrollToItem(panelIndex) }
        } else {
            onClose()
        }
    }

    val focusRequester = remember { FocusRequester() }
    // Focus the query field once per OPEN, not once per composition — the
    // same trap the selection reset above dodges, in the one place a key
    // cannot dodge it: a `LaunchedEffect` re-runs on Activity recreation
    // whatever it is keyed on, because the composition itself is new. So
    // the "already focused" fact has to survive the recreation, which is
    // what `rememberSaveable` does — and it is discarded when the overlay
    // leaves composition, so the next open focuses again. Without this, a
    // fold/unfold with the overlay open re-takes focus and re-opens the
    // IME over whatever the reader had scrolled to.
    var hasFocused by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        if (!hasFocused) {
            hasFocused = true
            focusRequester.requestFocus()
        }
    }

    Surface(
        modifier = Modifier.fillMaxSize(),
        color = MaterialTheme.colorScheme.background,
    ) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .statusBarsPadding()
                .navigationBarsPadding()
                .imePadding()
                .contentMaxWidth()
                .padding(start = 24.dp, top = 12.dp, end = 24.dp, bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                OutlinedTextField(
                    value = query,
                    onValueChange = { viewModel.setQueryText(it) },
                    label = { Text("Title, notes, project, or hb-42") },
                    modifier = Modifier
                        .weight(1f)
                        .focusRequester(focusRequester),
                )
                IconButton(onClick = onClose) {
                    Icon(
                        painterResource(R.drawable.ic_x),
                        contentDescription = "Close",
                        modifier = Modifier.size(18.dp),
                    )
                }
            }

            statusLine?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                )
            }

            val shown = rows.size.toUInt()
            val more = if (total > shown) total - shown else 0u

            when {
                query.isBlank() -> Text(
                    "Live, Done and archived items — everything the mirror has ever known.",
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                loading && rows.isEmpty() -> CircularProgressIndicator()
                rows.isEmpty() -> Text(
                    "Nothing matched. Every word has to appear, or type a handle like hb-42.",
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                else -> LazyColumn(
                    state = listState,
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    for (row in rows) {
                        item(key = row.id) {
                            RecallRow(
                                row = row,
                                selected = row.id == selectedId,
                                onClick = if (row.group == MobileRecallGroup.LIVE) {
                                    { viewModel.select(row.id) }
                                } else {
                                    null
                                },
                            )
                        }
                        // The web expansion, in place: the shared panel
                        // directly below its row, the list still standing.
                        if (row.id == selectedId) {
                            item(key = "detail-${row.id}") {
                                Card(
                                    modifier = Modifier.fillMaxWidth(),
                                    colors = CardDefaults.cardColors(
                                        containerColor = MaterialTheme.colorScheme.surface,
                                    ),
                                ) {
                                    ItemDetailPanel(
                                        itemId = row.id,
                                        syncTick = syncTick,
                                        closeLabel = "Close",
                                        onClose = { viewModel.clearSelection() },
                                        onGrill = onGrill,
                                        // An edit landing must repaint the row it
                                        // came from — re-ask the same query, the
                                        // web's own resolved-triage re-ask.
                                        onMutated = {
                                            scope.launch {
                                                viewModel.search(System.currentTimeMillis())
                                            }
                                        },
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .padding(12.dp),
                                    )
                                }
                            }
                        }
                    }
                    if (more > 0u) {
                        item {
                            Text(
                                "$more more matched — narrow the words to see them",
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
            }
        }
    }
}

private fun groupLabel(group: MobileRecallGroup): String = when (group) {
    MobileRecallGroup.LIVE -> "live"
    MobileRecallGroup.DONE -> "done"
    MobileRecallGroup.ARCHIVED -> "archived"
}

@Composable
private fun RecallRow(
    row: MobileRecallRowRecord,
    selected: Boolean,
    onClick: (() -> Unit)?,
) {
    // Exhaustive over the groups, no `else` arm (the structural test's
    // rule for a `uniffi::Enum` crossing): a fourth group added core-side
    // must be answered for here, not silently full-opacity.
    val alpha = when (row.group) {
        MobileRecallGroup.LIVE -> 1f
        MobileRecallGroup.DONE -> INERT_ALPHA
        MobileRecallGroup.ARCHIVED -> INERT_ALPHA
    }
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .alpha(alpha)
            .let { if (onClick != null) it.clickable(onClick = onClick) else it },
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        border = if (selected) {
            BorderStroke(1.dp, MaterialTheme.colorScheme.primary)
        } else {
            null
        },
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(12.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(row.title, style = MaterialTheme.typography.bodyLarge)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(
                        groupLabel(row.group),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    if (row.pending) {
                        Text(
                            "Pending",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
    }
}
