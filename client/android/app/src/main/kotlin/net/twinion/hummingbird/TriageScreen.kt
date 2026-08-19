package net.twinion.hummingbird

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.ui.ChoiceRow
import net.twinion.hummingbird.ui.LevelGlyphFamily
import net.twinion.hummingbird.ui.StageBadge
import net.twinion.hummingbird.ui.forms.CaptureDateField
import net.twinion.hummingbird.ui.forms.ContextField
import net.twinion.hummingbird.ui.forms.LevelSlider
import net.twinion.hummingbird.ui.forms.PriorityRow
import uniffi.hummingbird_ffi_mobile.CaptureFormMeta
import uniffi.hummingbird_ffi_mobile.MetaProblems
import uniffi.hummingbird_ffi_mobile.TriageItemRecord
import uniffi.hummingbird_ffi_mobile.itemGrillButtonLabel

// The Triage screen (#531): one queue holding both captured and Grilling
// items in the core's own order, headed by the two record-field counts
// (never recomputed here), one row open at a time into the seeded editor
// built from #529's shared form components, Promote-to-Ready the only
// destination this screen offers, and the row checkmark going through the
// existing `act` path — never a triage.
//
// **The Grill button is live (#539).** It navigates to the standalone
// takeover (`GrillTakeoverScreen.kt`) rather than opening an interview
// inline — this screen holds no turn/draft state of its own, gated on the
// row's own `canGrill`/`hasGrillDraft` facts from the seam.
@Composable
fun TriageScreen(
    syncTick: Int = 0,
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
    val dark = isSystemInDarkTheme()

    suspend fun reload() {
        viewModel.load()
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

    Scaffold { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("Triage", style = MaterialTheme.typography.headlineLarge)
                (state as? TriageState.Loaded)?.board?.let { board ->
                    // The two counts are the record's own fields — never a
                    // `board.items.size` recomputation, which is exactly
                    // this AC's point: `capturedCount`/`grillingCount` came
                    // decided across the seam.
                    Text(
                        "${board.capturedCount} captured · ${board.grillingCount} grilling",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
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

            when (val current = state) {
                TriageState.Loading -> CircularProgressIndicator()
                is TriageState.Loaded -> {
                    if (current.board.items.isEmpty()) {
                        Text(
                            "Nothing captured is waiting to be sorted.",
                            style = MaterialTheme.typography.bodyLarge,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    } else {
                        for (item in current.board.items) {
                            TriageRow(
                                item = item,
                                dark = dark,
                                expanded = selectedId == item.id,
                                draft = if (selectedId == item.id) draft else null,
                                formMeta = formMeta,
                                onToggle = { viewModel.select(item.id) },
                                onDraftChange = viewModel::updateDraft,
                                onComplete = {
                                    scope.launch {
                                        viewModel.complete(item.id, System.currentTimeMillis())
                                    }
                                },
                                onPromote = {
                                    scope.launch {
                                        viewModel.promote(item.id, System.currentTimeMillis())
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
        }
    }
}

@Composable
private fun TriageRow(
    item: TriageItemRecord,
    dark: Boolean,
    expanded: Boolean,
    draft: TriageDraft?,
    formMeta: CaptureFormMeta,
    onToggle: () -> Unit,
    onDraftChange: (TriageDraft) -> Unit,
    onComplete: () -> Unit,
    onPromote: () -> Unit,
    onGrill: () -> Unit,
    canSave: Boolean,
    metaProblems: MetaProblems?,
) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                // #576's fifth site, sighted alongside the four in the
                // issue: a long title took the whole width and left `Done`
                // squeezed to nothing. **Not the `ChoiceRow` remedy** — the
                // two are not peers to wrap, they are a title and its one
                // trailing action, so the title yields instead. `weight`
                // measures the unweighted `Done` first and hands the title
                // what is left; `fill = false` keeps a short title from
                // stretching, so the row still reads as SpaceBetween.
                TextButton(onClick = onToggle, modifier = Modifier.weight(1f, fill = false)) {
                    Column {
                        // One treatment per stage word (#557) —
                        // `ui/StageBadge.kt`. Stays inside the TextButton's
                        // Column so the row's tap target is unchanged.
                        StageBadge(stage = item.stage, dark = dark)
                        Text(item.title, style = MaterialTheme.typography.bodyLarge)
                    }
                }
                if (item.canMarkDone) {
                    OutlinedButton(onClick = onComplete) { Text("Done") }
                }
            }

            if (expanded && draft != null) {
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
    }
}
