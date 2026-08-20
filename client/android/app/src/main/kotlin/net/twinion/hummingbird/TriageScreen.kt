package net.twinion.hummingbird

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.ui.ChoiceRow
import net.twinion.hummingbird.ui.LevelGlyphFamily
import net.twinion.hummingbird.ui.StageBadge
import net.twinion.hummingbird.ui.contentMaxWidth
import net.twinion.hummingbird.ui.forms.CaptureDateField
import net.twinion.hummingbird.ui.forms.ContextField
import net.twinion.hummingbird.ui.forms.LevelSlider
import net.twinion.hummingbird.ui.forms.PriorityRow
import net.twinion.hummingbird.ui.theme.LocalHbDark
import uniffi.hummingbird_ffi_mobile.CaptureFormMeta
import uniffi.hummingbird_ffi_mobile.MetaProblems
import uniffi.hummingbird_ffi_mobile.TriageItemRecord
import uniffi.hummingbird_ffi_mobile.itemGrillButtonLabel

// The Triage screen (#531, reshaped by the Triage-parity slice): one queue
// holding both captured and Grilling items in the core's own order, headed
// by the two record-field counts (never recomputed here), rendered through
// the SAME compact card the Now screen's frontier uses (`NowRow.kt` — the
// operator request: same pills, same expansion shape). The selected item
// expands at index 0 of the one LazyColumn, above the queue, exactly the
// Now screen's inline-expansion pattern — except the expanded pane is the
// seeded triage editor, not `ItemDetailPanel`: `available_actions` answers
// nothing for Triage/Grilling stages, the panel's plain save is the
// non-promoting write this surface bans (#360), and the web's own capture
// expansion is the triage form. Promote-to-Ready stays the only
// destination, and the row checkmark still goes through the existing `act`
// path — never a triage.
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
    val draft by viewModel.draft.collectAsState()
    // Every size/energy/context word this screen's editor offers comes from
    // this door, never a Kotlin literal — the same vocabulary #529's
    // capture form already reads (`captureFormMeta`'s own doc: one source,
    // shared).
    val formMeta: CaptureFormMeta = viewModel.formMeta
    val dark = LocalHbDark.current
    val listState = rememberLazyListState()

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

    // Saveable: an Activity recreation mid-question must not silently
    // answer it (`ItemDetailPanel`'s own flag, same reason).
    var confirmingDiscard by rememberSaveable { mutableStateOf(false) }

    // System Back with typed edits open: the words a person wrote are never
    // thrown away silently — the house rule `ItemDetailPanel`'s header
    // states, and this editor had no guard at all. Leaving really is the end
    // of the draft here: a bar-tab pop carries no `saveState` for this
    // entry, so the ViewModel holding it retires with the entry.
    //
    // Registered at the screen, not inside the editor's LazyColumn item: an
    // item scrolled out of the viewport is DISPOSED, taking any handler it
    // registered with it (`NowScreen`'s own guard exists for exactly that).
    // Only while there is something to lose — an idle Back is never fought,
    // and still pops the entry the way it always did.
    BackHandler(enabled = draft != null && viewModel.isDirty) {
        confirmingDiscard = true
    }

    if (confirmingDiscard) {
        DiscardConfirmation(
            onKeep = { confirmingDiscard = false },
            // Discarding closes the editor and stays on Triage — the Back
            // that asked the question is spent on the answer, exactly what
            // the panel's own discard does.
            onDiscard = {
                confirmingDiscard = false
                viewModel.discardDraft()
            },
        )
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

                    selectedId?.let { id ->
                        val item = board?.items?.find { it.id == id }
                        val openDraft = draft
                        if (item != null && openDraft != null) {
                            item(key = "selected-item") {
                                Card(
                                    modifier = Modifier.fillMaxWidth(),
                                    colors = CardDefaults.cardColors(
                                        containerColor = MaterialTheme.colorScheme.surface,
                                    ),
                                ) {
                                    TriageEditorPanel(
                                        item = item,
                                        dark = dark,
                                        draft = openDraft,
                                        formMeta = formMeta,
                                        onDraftChange = viewModel::updateDraft,
                                        // The × is the same leaving gesture
                                        // Back is, so it asks the same
                                        // question: closing on a dirty draft
                                        // routes through the confirmation
                                        // rather than dropping typed words.
                                        onClose = {
                                            if (viewModel.isDirty) {
                                                confirmingDiscard = true
                                            } else {
                                                viewModel.select(id)
                                            }
                                        },
                                        onPromote = {
                                            scope.launch {
                                                viewModel.promote(
                                                    item.id,
                                                    nowDeadlineShaped(),
                                                    System.currentTimeMillis(),
                                                )
                                            }
                                        },
                                        onGrill = { onGrill(item.id) },
                                        canSave = viewModel.canSave,
                                        metaProblems = viewModel.metaProblems,
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

/** The opened capture's editor, in the Now panel's chrome — a header row
 * (stage chip, the title at the panel's own titleMedium, an X close) over
 * the seeded field set, ending in the Grill/Promote choice row. The fields
 * and their rules are #529's shared form components, unchanged from the
 * inline-expansion form this panel replaces; only the placement moved (to
 * index 0 of the queue's LazyColumn, `NowScreen`'s inline-expansion
 * pattern). */
@Composable
private fun TriageEditorPanel(
    item: TriageItemRecord,
    dark: Boolean,
    draft: TriageDraft,
    formMeta: CaptureFormMeta,
    onDraftChange: (TriageDraft) -> Unit,
    onClose: () -> Unit,
    onPromote: () -> Unit,
    onGrill: () -> Unit,
    canSave: Boolean,
    metaProblems: MetaProblems?,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                StageBadge(stage = item.stage, dark = dark)
                Text(item.title, style = MaterialTheme.typography.titleMedium)
            }
            IconButton(onClick = onClose) {
                Icon(
                    painterResource(R.drawable.ic_x),
                    contentDescription = "Close",
                    modifier = Modifier.size(18.dp),
                )
            }
        }

        OutlinedTextField(
            value = draft.title,
            onValueChange = { onDraftChange(draft.copy(title = it)) },
            label = { Text("Title") },
            isError = !canSave && draft.title.isEmpty(),
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = draft.description,
            onValueChange = { onDraftChange(draft.copy(description = it)) },
            label = { Text("Notes") },
            modifier = Modifier.fillMaxWidth(),
        )
        LevelSlider(
            label = "Energy",
            glyphFamily = LevelGlyphFamily.ENERGY,
            options = formMeta.energies,
            selected = draft.energy.ifEmpty { null },
            onSelect = { onDraftChange(draft.copy(energy = it.orEmpty())) },
        )
        LevelSlider(
            label = "Size",
            glyphFamily = LevelGlyphFamily.SIZE,
            options = formMeta.sizes,
            selected = draft.size.ifEmpty { null },
            onSelect = { onDraftChange(draft.copy(size = it.orEmpty())) },
        )
        // #565's review: `TriageDraft.priority` seeds from the row
        // and `toEdit` sends it, so without this control the field
        // was carried through every promote as state nothing could
        // reach. The same shared component the capture box renders,
        // for the same reason `LevelSlider` is shared.
        PriorityRow(
            selected = draft.priority,
            onSelect = { onDraftChange(draft.copy(priority = it)) },
        )
        ContextField(
            value = draft.context,
            onValueChange = { onDraftChange(draft.copy(context = it)) },
            suggestions = formMeta.suggestedContexts,
        )
        CaptureDateField(
            label = "Deadline",
            value = draft.deadline,
            error = metaProblems?.deadline,
            onValueChange = { onDraftChange(draft.copy(deadline = it)) },
        )
        CaptureDateField(
            label = "Scheduled date",
            value = draft.scheduledDate,
            error = metaProblems?.scheduledDate,
            onValueChange = { onDraftChange(draft.copy(scheduledDate = it)) },
        )

        // #576: these two fit today and were never sighted failing,
        // but they are the same shape as the three rows that did —
        // and `Resume grill` is wider than `Grill me`, so the
        // margin is not the one it looks like.
        ChoiceRow {
            // Live (#539): navigates to the standalone takeover.
            // `item.canGrill` is the seam's own decided fact — both
            // Triage and Grilling rows on this board offer it.
            if (item.canGrill) {
                OutlinedButton(onClick = onGrill) {
                    Text(itemGrillButtonLabel(item.hasGrillDraft))
                }
            }
            // Promoting to Ready is the only destination this
            // screen offers — the web triage panel's own
            // "Promote to ready", nothing beside it.
            Button(onClick = onPromote, enabled = canSave) {
                Text("Promote to ready")
            }
        }
    }
}
