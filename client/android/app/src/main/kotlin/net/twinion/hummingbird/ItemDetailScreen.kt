package net.twinion.hummingbird

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
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
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.skills.BackendPreference
import net.twinion.hummingbird.ui.ChoiceRow
import net.twinion.hummingbird.ui.EnergyGlyph
import net.twinion.hummingbird.ui.SizeGlyph
import net.twinion.hummingbird.ui.StageBadge
import net.twinion.hummingbird.ui.levelColor
import net.twinion.hummingbird.ui.levelPosition
import uniffi.hummingbird_ffi_mobile.ItemDetailRecord
import uniffi.hummingbird_ffi_mobile.MetaProblems
import uniffi.hummingbird_ffi_mobile.MobileMicrotaskAffordance
import uniffi.hummingbird_ffi_mobile.MobileSkillRunState
import uniffi.hummingbird_ffi_mobile.itemCanGrill
import uniffi.hummingbird_ffi_mobile.itemGrillButtonLabel
import uniffi.hummingbird_ffi_mobile.skillRunStampLabel

// One item, in full (#141's last slice, ADR-0027) — where a tapped
// `item-threshold/v1` notification lands, because a state source's alert is
// a reading of the item's condition and landing on the alert would show a
// reading of a thing while withholding the thing.
//
// This file decides nothing. `availableActions` gates the act buttons,
// `isEditable` gates the edit affordance, `canAck` gates the Ack, and the
// open-blocker list is rendered exactly as the core assembled it — a
// titleless row means an id this device has not synced, and it is shown as
// the bare id rather than dropped, because a count that understates what
// holds an item back is worse than an ugly row.
//
// **The one dialog in the app, and why.** The house is dialog-wary
// (`AlertsScreen`'s own note). This one guards a draft: content a person
// wrote by hand. The repo's standing rule — the dead-letter journal keeps
// the words a person wrote, "parse is additive" never discards input — is
// that human-authored content is never silently thrown away, and Back out
// of a changed edit form would do exactly that. An *unchanged* draft is
// never fought over: the `BackHandler` is conditional on the draft
// actually differing from the record.

/** The two level vocabularies, in the core's own order — one copy serving
 * read mode's glyph positions and edit mode's `VocabularyRow`s alike.
 * Order is what `levelPosition` indexes by (#558), and it is pinned against
 * the core by `the_now_screen_facet_vocabularies_match_the_core`
 * (`ffi-mobile`); this file is deliberately outside
 * `CaptureFieldSetStructuralTest`'s literal ban, which guards the capture
 * form's own files. */
private val SIZE_VOCABULARY = listOf("quick", "normal", "deep")
private val ENERGY_VOCABULARY = listOf("low", "medium", "high")

@Composable
fun ItemDetailScreen(
    itemId: String,
    syncTick: Int = 0,
    onBack: () -> Unit,
    onGrill: (String) -> Unit = {},
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val viewModel: ItemDetailViewModel =
        viewModel(factory = ItemDetailViewModel.factory(context))
    val microtaskViewModel: MicrotaskViewModel =
        viewModel(factory = MicrotaskViewModel.factory(context))
    val state by viewModel.state.collectAsState()
    val statusLine by viewModel.statusLine.collectAsState()
    val draft by viewModel.draft.collectAsState()
    val microtaskRun by microtaskViewModel.run.collectAsState()
    val microtaskDeclinedFallbackId by microtaskViewModel.declinedFallbackId.collectAsState()
    val microtaskDeclinedFallbackLabel = microtaskDeclinedFallbackId?.let { id ->
        BackendPreference.ENTRIES.find { it.id == id }?.label ?: id
    }
    val hasGrillDraft by viewModel.hasGrillDraft.collectAsState()
    val dark = isSystemInDarkTheme()
    // Saveable: an Activity recreation mid-question must not silently
    // answer it.
    var confirmingDiscard by rememberSaveable { mutableStateOf(false) }

    suspend fun reload() {
        viewModel.load(itemId, System.currentTimeMillis())
    }

    LaunchedEffect(itemId) { reload() }

    LifecycleResumeEffect(itemId) {
        val resumed = scope.launch { reload() }
        onPauseOrDispose { resumed.cancel() }
    }

    LaunchedEffect(syncTick) {
        if (syncTick > 0) reload()
    }

    // A microtask run's own cycle (#565 review). It drives the core
    // directly, so `syncTick` above never moves for it and the new steps —
    // and the affordance they change — would wait for the 60-second
    // cadence tick. `MicrotaskViewModel.syncedTick` is that run's
    // completion, published only after its sync returned.
    val microtaskSyncedTick by microtaskViewModel.syncedTick.collectAsState()
    LaunchedEffect(microtaskSyncedTick) {
        if (microtaskSyncedTick > 0) reload()
    }

    // Only while there is something to lose. An idle Back is never fought.
    BackHandler(enabled = draft != null && viewModel.isDirty) {
        confirmingDiscard = true
    }

    if (confirmingDiscard) {
        DiscardConfirmation(
            onKeep = { confirmingDiscard = false },
            onDiscard = {
                confirmingDiscard = false
                viewModel.discardEdit()
            },
        )
    }

    Scaffold { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            TextButton(
                onClick = {
                    if (draft != null && viewModel.isDirty) confirmingDiscard = true
                    else if (draft != null) viewModel.discardEdit()
                    else onBack()
                },
            ) {
                Text(if (draft != null) "Cancel" else "Back")
            }

            statusLine?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                )
            }

            when (val current = state) {
                ItemDetailState.Loading -> CircularProgressIndicator()
                ItemDetailState.NotSynced -> ItemNotSyncedBody(
                    onRetry = { scope.launch { reload() } },
                )
                is ItemDetailState.Loaded -> {
                    val editing = draft
                    if (editing == null) {
                        ReadBody(
                            record = current.record,
                            dark = dark,
                            onEdit = { viewModel.beginEdit() },
                            onAct = { action ->
                                scope.launch {
                                    viewModel.act(itemId, action, System.currentTimeMillis())
                                }
                            },
                            onAck = { alertId ->
                                scope.launch {
                                    viewModel.ack(itemId, alertId, System.currentTimeMillis())
                                }
                            },
                            onGrill = { onGrill(itemId) },
                            hasGrillDraft = hasGrillDraft,
                            microtaskRun = microtaskRun,
                            // The current backend selection resolves to a
                            // `model` INSIDE `MicrotaskViewModel.run` (off
                            // `BackendPreference`, #274's Settings picker)
                            // — never a literal here.
                            onMicrotaskRun = { replace, grain -> microtaskViewModel.run(itemId, replace, grain) },
                            declinedFallbackLabel = microtaskDeclinedFallbackLabel,
                            onSwitchAndRetry = { microtaskViewModel.switchAndRetry() },
                        )
                    } else {
                        EditBody(
                            draft = editing,
                            problems = viewModel.metaProblems,
                            canSave = viewModel.canSave,
                            onChange = viewModel::updateDraft,
                            onSave = {
                                scope.launch {
                                    viewModel.save(itemId, System.currentTimeMillis())
                                }
                            },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun ItemNotSyncedBody(onRetry: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("Not synced yet", style = MaterialTheme.typography.headlineLarge)
        Text(
            "This device hasn't got this item yet. A sync has already run " +
                "once; try again in a moment.",
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        OutlinedButton(onClick = onRetry) {
            Text("Try again")
        }
    }
}

@Composable
private fun ReadBody(
    record: ItemDetailRecord,
    dark: Boolean,
    onEdit: () -> Unit,
    onAct: (String) -> Unit,
    onAck: (String) -> Unit,
    onGrill: () -> Unit,
    hasGrillDraft: Boolean,
    microtaskRun: MobileSkillRunState,
    onMicrotaskRun: (replace: Boolean, grain: Long?) -> Unit,
    declinedFallbackLabel: String?,
    onSwitchAndRetry: () -> Unit,
) {
    if (record.isArchived) {
        // Honesty over reassurance: history is readable here, and says so.
        Text(
            "ARCHIVED · READ ONLY",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier
                .background(
                    MaterialTheme.colorScheme.surfaceVariant,
                    RoundedCornerShape(6.dp),
                )
                .padding(horizontal = 8.dp, vertical = 2.dp),
        )
    }

    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        // One treatment per stage word (#557) — `ui/StageBadge.kt`, never
        // a raw `stage.uppercase()`.
        StageBadge(stage = record.stage, dark = dark)
        record.seq?.let {
            Text(
                "HB-$it",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        record.projectName?.let {
            Text(
                it,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } ?: record.projectId?.let {
            // The name is unsynced, not the project: show the id rather
            // than pretending the item belongs to nothing.
            Text(
                it,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }

    Text(record.title, style = MaterialTheme.typography.headlineLarge)

    record.description?.let {
        Text(it, style = MaterialTheme.typography.bodyLarge)
    }

    // The four axes and the two dates, in the mono meta style: values the
    // system holds, not words a human wrote. Size and energy are drawn as
    // well as written (#558, ADR-0024) — glyph beside word, one ramp colour
    // over both — and this surface has the room the ADR requires, so it is
    // the ONE place the unset ghost renders (position 0 beside an em dash):
    // `size-unset` and `size-deep` are the same three rings told apart by
    // opacity alone, which is why nothing word-free ever draws a ghost.
    FlowRow(
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        val sizePos = levelPosition(SIZE_VOCABULARY, record.size)
        val sizeColor = levelColor(sizePos, dark)
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            SizeGlyph(position = sizePos, color = sizeColor, size = 13.dp)
            Text(
                "SIZE:${record.size?.uppercase() ?: "—"}",
                style = MaterialTheme.typography.labelSmall,
                color = sizeColor,
            )
        }
        val energyPos = levelPosition(ENERGY_VOCABULARY, record.energy)
        val energyColor = levelColor(energyPos, dark)
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            EnergyGlyph(position = energyPos, color = energyColor, size = 13.dp)
            Text(
                "ENERGY:${record.energy?.uppercase() ?: "—"}",
                style = MaterialTheme.typography.labelSmall,
                color = energyColor,
            )
        }
        val axes = buildList {
            record.context?.let { add(it.uppercase()) }
            if (record.agent) add("AGENT")
            add("PRIORITY:${record.priority}")
            record.deadline?.let { add("DUE:$it") }
            record.scheduledDate?.let { add("SCHEDULED:$it") }
        }
        Text(
            axes.joinToString(" · "),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }

    if (record.openBlockers.isNotEmpty()) {
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(
                "BLOCKED BY",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            for (blocker in record.openBlockers) {
                // A titleless blocker renders as its id — never dropped.
                Text(
                    blocker.title ?: blocker.itemId,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
        }
    }

    if (record.steps.isNotEmpty()) {
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(
                "STEPS",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            for (step in record.steps) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    // Read-only: no step mutation crosses the seam yet, and
                    // a tickable-looking checkbox that did nothing would be
                    // a lie about what this build can do.
                    Text(
                        if (step.done) "DONE" else "TODO",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Text(step.body, style = MaterialTheme.typography.bodyMedium)
                }
            }
        }
    }

    record.liveAlert?.let { alert ->
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(
                containerColor = MaterialTheme.colorScheme.surfaceVariant,
            ),
        ) {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    val (fg, bg) = severityTone(alert.severity, dark)
                    Text(
                        severityLabel(alert.severity),
                        style = MaterialTheme.typography.labelSmall,
                        color = fg,
                        modifier = Modifier
                            .background(bg, RoundedCornerShape(6.dp))
                            .padding(horizontal = 8.dp, vertical = 2.dp),
                    )
                }
                // Suppressed when it would restate the heading directly
                // above it (#522). Not a quirk of one alert: `sweep.rs`
                // builds an `item-threshold/v1` ingest with
                // `title: item.title`, so for this source -- the one this
                // screen exists to land (ADR-0027) -- the two are always
                // the same string. The severity pill and the Ack carry the
                // card without it.
                //
                // Conditional rather than removed: the card also renders
                // for a future source whose title says something the
                // heading does not, and dropping the line outright would
                // lose that.
                if (alert.title != record.title) {
                    Text(alert.title, style = MaterialTheme.typography.bodyLarge)
                }
                alert.body?.let {
                    Text(
                        it,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                // The Ack lives here, one tap from where the notification
                // landed (ADR-0027 part 3) — and stays offered on an
                // archived item, since silencing something still ringing is
                // not editing history.
                if (alert.canAck) {
                    Button(onClick = { onAck(alert.id) }) {
                        Text("Ack")
                    }
                }
            }
        }
    }

    // #576: the same four actions `NowScreen`'s card offers, and the same
    // reason it wraps them — "Start / Complete / Mark blocked / Cancel" is
    // wider than a phone, and a plain `Row` left `Cancel` a column of
    // letters rather than a button.
    ChoiceRow {
        for (action in record.availableActions) {
            OutlinedButton(onClick = { onAct(action) }) {
                Text(ACTION_LABEL[action] ?: action)
            }
        }
    }

    // Live (#539): `itemCanGrill` is the seam's own rule, the same one
    // `TriageItemRecord.canGrill` reads per row. `isEditable` is gated
    // alongside it, and cannot be folded into `itemCanGrill(record.stage)`
    // — `stage` alone cannot tell a cancelled item from a live one: `Core
    // ::act`'s cancel sets `archivedAt`, never `stage`, so a cancelled
    // Ready/In Progress item still carries a `canGrill`-eligible stage.
    // Without this second check, that archived row would offer a live
    // "Grill me" whose Confirm could still enqueue a `CompleteGrill` on
    // history (the same recall rule `record.isEditable` already gates Edit
    // and the microtask affordance on).
    if (record.isEditable && itemCanGrill(record.stage)) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = onGrill) {
                Text(itemGrillButtonLabel(hasGrillDraft))
            }
        }
    }

    // #539's microtask affordance: an *applied result*, not a re-derived
    // one — `record.microtaskAffordance` is `null` for a non-editable
    // (archived) item and `Break`/`Rewrite` otherwise, decided by
    // `hummingbird_core::decisions::skills::microtask_affordance`. This
    // block offers nothing of its own eligibility logic.
    record.microtaskAffordance?.let { affordance ->
        MicrotaskSection(
            affordance = affordance,
            run = microtaskRun,
            onRun = onMicrotaskRun,
            declinedFallbackLabel = declinedFallbackLabel,
            onSwitchAndRetry = onSwitchAndRetry,
        )
    }

    if (record.isEditable) {
        OutlinedButton(onClick = onEdit) {
            Text("Edit")
        }
    }
}

/** The microtask affordance's own render — narrates as it streams, and a
 * decline is shown verbatim (#539's own AC), never paraphrased: #307 made
 * the seam's decline prose-only, with no reason code, precisely so nothing
 * string-matches it, here as on the web. */
@Composable
private fun MicrotaskSection(
    affordance: MobileMicrotaskAffordance,
    run: MobileSkillRunState,
    onRun: (replace: Boolean, grain: Long?) -> Unit,
    declinedFallbackLabel: String?,
    onSwitchAndRetry: () -> Unit,
) {
    var grain by rememberSaveable { mutableStateOf(2L) }
    val running = run is MobileSkillRunState.Running

    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Button(
                enabled = !running,
                onClick = {
                    when (affordance) {
                        MobileMicrotaskAffordance.Break -> onRun(false, null)
                        is MobileMicrotaskAffordance.Rewrite -> onRun(true, grain)
                    }
                },
            ) {
                Text(
                    when (affordance) {
                        MobileMicrotaskAffordance.Break -> "Break into steps"
                        is MobileMicrotaskAffordance.Rewrite ->
                            "Rewrite ${affordance.undoneCount} step" +
                                if (affordance.undoneCount == 1u) "" else "s"
                    },
                )
            }
        }

        if (run !is MobileSkillRunState.Idle) {
            val messages = when (run) {
                MobileSkillRunState.Idle -> emptyList()
                is MobileSkillRunState.Running -> run.messages
                is MobileSkillRunState.Done -> run.messages
                is MobileSkillRunState.Declined -> run.messages
            }
            Narration(messages)

            val stamp = skillRunStampLabel(run)
            if (stamp != null) {
                Text(stamp, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }

            if (run is MobileSkillRunState.Done && run.note.isNotEmpty()) {
                Text(run.note, style = MaterialTheme.typography.bodyMedium)
            }

            // Verbatim, unprefixed, unbranched.
            if (run is MobileSkillRunState.Declined) {
                Text(run.reason, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.error)
                // #274: a pinned, dead backend is never silently rerouted —
                // this is the one-tap offer, not an automatic fallback.
                // Absent whenever the current selection is Auto, the
                // decline names no reachability problem, or the registry
                // has nothing else to try (`declinedBackendFallback`'s own
                // doc states all four exclusions).
                if (declinedFallbackLabel != null) {
                    OutlinedButton(onClick = onSwitchAndRetry) {
                        Text("Switch to $declinedFallbackLabel")
                    }
                }
            }
        }
    }
}

@Composable
private fun Narration(messages: List<String>) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        for (message in messages) {
            Text(
                message,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun EditBody(
    draft: ItemDraft,
    problems: MetaProblems?,
    canSave: Boolean,
    onChange: (ItemDraft) -> Unit,
    onSave: () -> Unit,
) {
    OutlinedTextField(
        value = draft.title,
        onValueChange = { onChange(draft.copy(title = it)) },
        label = { Text("Title") },
        // An item must have a title (`NOT NULL`), so an empty one is
        // refused here rather than saved as a silent no-op. Whether it
        // *is* empty is the core's answer, never this file's.
        isError = !canSave && draft.title.isEmpty(),
        modifier = Modifier.fillMaxWidth(),
    )
    OutlinedTextField(
        value = draft.description,
        onValueChange = { onChange(draft.copy(description = it)) },
        label = { Text("Notes") },
        modifier = Modifier.fillMaxWidth(),
    )
    OutlinedTextField(
        value = draft.context,
        onValueChange = { onChange(draft.copy(context = it)) },
        // Free text, not a picker: the set of places a person works is
        // theirs (CONTEXT.md's Context — an open vocabulary).
        label = { Text("Context") },
        modifier = Modifier.fillMaxWidth(),
    )
    // The two free-text dates are the only fields that can be malformed —
    // everything else is a closed vocabulary offered as choices, or the
    // title. The problem strings are the core's, shared with the web's
    // capture box and triage form, so a bad date is refused with the same
    // words everywhere instead of being sent for the authority to 400.
    OutlinedTextField(
        value = draft.deadline,
        onValueChange = { onChange(draft.copy(deadline = it)) },
        label = { Text("Deadline") },
        isError = problems?.deadline != null,
        supportingText = problems?.deadline?.let { { Text(it) } },
        modifier = Modifier.fillMaxWidth(),
    )
    OutlinedTextField(
        value = draft.scheduledDate,
        onValueChange = { onChange(draft.copy(scheduledDate = it)) },
        label = { Text("Scheduled date") },
        isError = problems?.scheduledDate != null,
        supportingText = problems?.scheduledDate?.let { { Text(it) } },
        modifier = Modifier.fillMaxWidth(),
    )

    // Closed vocabularies, so a choice row rather than a text field: an
    // unrecognised word is refused at the seam, and offering one to type
    // would invite exactly that refusal.
    VocabularyRow(
        label = "SIZE",
        options = SIZE_VOCABULARY,
        selected = draft.size,
        onSelect = { onChange(draft.copy(size = it)) },
    )
    VocabularyRow(
        label = "ENERGY",
        options = ENERGY_VOCABULARY,
        selected = draft.energy,
        onSelect = { onChange(draft.copy(energy = it)) },
    )
    VocabularyRow(
        label = "PRIORITY",
        options = listOf("0", "1", "2", "3", "4"),
        selected = draft.priority,
        // Priority is `NOT NULL`: re-tapping the current value cannot
        // clear it, so this row never sends an empty string.
        onSelect = { onChange(draft.copy(priority = it)) },
        clearable = false,
    )

    Button(onClick = onSave, enabled = canSave) {
        Text("Save")
    }
}

/** One closed-vocabulary field as a row of words. Tapping the selected
 * word again clears the field — which is a real edit (an explicit null),
 * distinct from leaving it alone, and the only way to say "this item has
 * no size after all". */
@Composable
private fun VocabularyRow(
    label: String,
    options: List<String>,
    selected: String,
    onSelect: (String) -> Unit,
    clearable: Boolean = true,
) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(
            label,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (option in options) {
                val isSelected = option == selected
                if (isSelected) {
                    Button(onClick = { if (clearable) onSelect("") }) {
                        Text(option)
                    }
                } else {
                    OutlinedButton(onClick = { onSelect(option) }) {
                        Text(option)
                    }
                }
            }
        }
    }
}

/** The discard confirmation — the app's first dialog, and deliberately its
 * only one. See this file's header for why a draft earns it. */
@Composable
private fun DiscardConfirmation(onKeep: () -> Unit, onDiscard: () -> Unit) {
    AlertDialog(
        onDismissRequest = onKeep,
        title = { Text("Discard changes?") },
        text = {
            Text("This edit hasn't been saved. Going back will lose it.")
        },
        confirmButton = {
            TextButton(onClick = onDiscard) { Text("Discard") }
        },
        dismissButton = {
            TextButton(onClick = onKeep) { Text("Keep editing") }
        },
    )
}
