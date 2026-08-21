package net.twinion.hummingbird

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.skills.BackendPreference
import net.twinion.hummingbird.ui.ChoiceRow
import net.twinion.hummingbird.ui.EnergyGlyph
import net.twinion.hummingbird.ui.LevelGlyphFamily
import net.twinion.hummingbird.ui.SizeGlyph
import net.twinion.hummingbird.ui.StageBadge
import net.twinion.hummingbird.ui.forms.CaptureDateField
import net.twinion.hummingbird.ui.forms.ContextField
import net.twinion.hummingbird.ui.forms.LevelSlider
import net.twinion.hummingbird.ui.forms.PriorityRow
import net.twinion.hummingbird.ui.levelColor
import net.twinion.hummingbird.ui.levelPosition
import net.twinion.hummingbird.ui.theme.LocalHbDark
import net.twinion.hummingbird.ui.theme.Moss600
import net.twinion.hummingbird.ui.theme.StatusDoneFgDark
import uniffi.hummingbird_ffi_mobile.CaptureFormMeta
import uniffi.hummingbird_ffi_mobile.ItemDetailRecord
import uniffi.hummingbird_ffi_mobile.MetaProblems
import uniffi.hummingbird_ffi_mobile.MobileMicrotaskAffordance
import uniffi.hummingbird_ffi_mobile.MobileSkillRunState
import uniffi.hummingbird_ffi_mobile.itemCanGrill
import uniffi.hummingbird_ffi_mobile.itemGrillButtonLabel
import uniffi.hummingbird_ffi_mobile.skillRunStampLabel

// One item, in full — the panel every one of its four hosts renders:
//
//  - the `ItemDetailScreen` route (#141's last slice, ADR-0027 — where a
//    tapped `item-threshold/v1` notification lands, because a state
//    source's alert is a reading of the item's condition and landing on the
//    alert would show a reading of a thing while withholding the thing),
//  - `NowScreen`'s inline expansion (a tapped card opens above the
//    still-standing board, `NowScreen.tsx`'s own `SelectedItemSection` /
//    ADR-0021 decision 7),
//  - the `RecallOverlay`'s expansion under its result row,
//  - and `TriageScreen`'s expansion at index 0 of its queue, in
//    [ItemDetailPanelMode.PROMOTE].
//
// Triage rendered a separate seeded editor until the unification: two
// panels that disagreed about the header, the field widgets and the
// mark-done check, with a duplicated draft type behind them. One panel and
// one draft is the whole point — the mode below is the only thing that
// differs between hosts.
//
// This file decides nothing. `availableActions` gates the act buttons,
// `canMarkDone` gates the check, `isEditable` gates every editable row
// and the submit, `canAck` gates the Ack, and the open-blocker list is rendered
// exactly as the core assembled it — a titleless row means an id this
// device has not synced, and it is shown as the bare id rather than
// dropped, because a count that understates what holds an item back is
// worse than an ugly row.
//
// **Scroll belongs to the host.** The route host wraps this panel's
// `modifier` in its own `verticalScroll`; the three inline hosts sit inside
// their screen's one `LazyColumn` and must NOT — two same-direction
// scrollables nest into a measurement crash, so the panel itself never
// scrolls.
//
// **State is keyed per item.** Both `viewModel(...)` calls carry
// `key = "…-$itemId"` so a different item is a different ViewModel pair —
// the inline hosts swap items without navigating, and an unkeyed lookup
// would hand item B item A's draft. Every piece of *composition* state here
// is keyed the same way (`rememberSaveable(itemId)`): under a constant
// LazyColumn key the composable is not disposed between selections, so an
// unkeyed flag leaks — title-edit mode used to open by itself on the next
// item selected, and the microtask grain carried over with it.
//
// The cost of the keyed ViewModels is one retired pair per item expanded
// this session, held by the host's back-stack entry and reclaimed when it
// pops; accepted (the web remounts `ItemPanel` per selection for the same
// reason, `NowScreen.tsx`'s own comment).
//
// **The one dialog in the app, and why.** The house is dialog-wary
// (`AlertsScreen`'s own note). This one guards a draft: content a person
// wrote by hand. The repo's standing rule — the dead-letter journal keeps
// the words a person wrote, "parse is additive" never discards input — is
// that human-authored content is never silently thrown away, and Back out
// of a changed draft would do exactly that. An *unchanged* draft is never
// fought over: the `BackHandler` is conditional on the draft actually
// differing from what it was seeded with.

/** Which host is rendering, and it decides exactly three things: the
 * submit's word, which ViewModel method that submit fires, and whether a
 * field the item has no value for opens editable or rests as an em-dash
 * ghost.
 *
 * **[PROMOTE] is #360's enforcement.** That issue bans a non-promoting
 * write from the Triage surface, and [ItemDetailViewModel] — shared with
 * the other three hosts — still carries `save`. So the ban is upheld here,
 * by this mode choosing `promote`, and pinned structurally
 * (`ItemDetailPanelStructuralTest`: the factory's literal
 * `triageItem(itemId, true,` and the absence of any `.save(` in Triage's
 * own sources). */
enum class ItemDetailPanelMode {
    /** Now, the notification route and the Recall overlay: the submit is
     * "Save", and an unset field rests as a ghost rather than opening a
     * form on a surface that is mostly for reading. */
    SAVE,

    /** Triage: the submit is "Promote", and an unset field opens
     * editable, because filling those in is the work this queue exists
     * for. */
    PROMOTE,
}

/** S11/#109's wire vocabulary, mapped to its button label and nothing more
 * — `ItemPanel.tsx`'s `ACTION_BUTTON` verbatim (`client/web/src/components/
 * domain/ItemPanel.tsx:74`; `Mark blocked` says what `Blocked` means, per
 * the design README's voice rule; "Blocked" means an external wait and
 * nothing else). Which actions an item *offers* is decided entirely
 * core-side ([ItemDetailRecord.availableActions]) — this map only ever
 * renders whatever that list already contains.
 *
 * `complete` is in the map and is deliberately never rendered from it: the
 * panel filters it out of the button row because the green check at the
 * panel's foot is that gesture, drawn once. The entry stays because the map
 * is the vocabulary's labels, not the button row's contents. */
internal val ACTION_LABEL: Map<String, String> = mapOf(
    "start" to "Start",
    "complete" to "Complete",
    "block" to "Mark blocked",
    "cancel" to "Cancel",
)

@Composable
fun ItemDetailPanel(
    itemId: String,
    syncTick: Int,
    /** The close control's accessible word — "Back" from the route,
     * "Close" inline. */
    closeLabel: String,
    onClose: () -> Unit,
    onGrill: (String) -> Unit,
    /** Fired after an act, Ack or submit lands — the inline hosts re-read
     * their board so the mutation shows behind the panel immediately; the
     * route host has no board and passes nothing. */
    onMutated: () -> Unit = {},
    /** Fired after a submit or a mark-done **lands** — the gestures that
     * can take the item out of the host's own list. Triage closes its
     * selection on it, or `selectedId` would dangle at a vanished row.
     *
     * Only on one that landed: a refused or failed write words itself into
     * the pane's own status line, and closing the pane would unmount both
     * that message and the draft it is about. */
    onSubmitted: () -> Unit = {},
    mode: ItemDetailPanelMode = ItemDetailPanelMode.SAVE,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val viewModel: ItemDetailViewModel =
        viewModel(factory = ItemDetailViewModel.factory(context), key = "item-$itemId")
    val microtaskViewModel: MicrotaskViewModel =
        viewModel(factory = MicrotaskViewModel.factory(context), key = "microtask-$itemId")
    val state by viewModel.state.collectAsState()
    val statusLine by viewModel.statusLine.collectAsState()
    val draft by viewModel.draft.collectAsState()
    val microtaskRun by microtaskViewModel.run.collectAsState()
    val microtaskDeclinedFallbackId by microtaskViewModel.declinedFallbackId.collectAsState()
    val microtaskDeclinedFallbackLabel = microtaskDeclinedFallbackId?.let { id ->
        BackendPreference.ENTRIES.find { it.id == id }?.label ?: id
    }
    val hasGrillDraft by viewModel.hasGrillDraft.collectAsState()
    val dark = LocalHbDark.current
    // Saveable: an Activity recreation mid-question must not silently
    // answer it.
    var confirmingDiscard by rememberSaveable { mutableStateOf(false) }
    // Keyed on the item: under a constant LazyColumn key this composable
    // survives a selection change, so an unkeyed flag would open the next
    // item's title field by itself.
    var editingTitle by rememberSaveable(itemId) { mutableStateOf(false) }
    val titleFocus = remember { FocusRequester() }
    // **Keyed on whether the field is actually there, not on the flag
    // alone.** `editingTitle` is restored per item, so reopening a pane
    // that was left mid-title-edit composes with the flag already `true`
    // while the record is still loading — and the field only renders once
    // there is a draft to bind, so the request would have no target and
    // `requestFocus()` throws `FocusRequester is not initialized`. That
    // crash was sighted on hardware and is invisible to every JVM test
    // here. Keying on the same condition the field renders on fires the
    // effect in the composition that places it, and never before.
    //
    // Keyed at all (rather than `LaunchedEffect(Unit)`) because #634 found
    // that shape re-firing on Activity recreation and undoing state.
    val titleFieldOpen = editingTitle && draft != null
    LaunchedEffect(titleFieldOpen) {
        if (titleFieldOpen) titleFocus.requestFocus()
    }

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

    // Every leaving gesture — Back, the X, the header tap — asks the same
    // question, and only when there is something to lose.
    fun requestClose() {
        if (viewModel.isDirty) confirmingDiscard = true else onClose()
    }

    // Only while there is something to lose. An idle Back is never fought.
    BackHandler(enabled = viewModel.isDirty) {
        confirmingDiscard = true
    }

    if (confirmingDiscard) {
        DiscardConfirmation(
            onKeep = { confirmingDiscard = false },
            onDiscard = {
                confirmingDiscard = false
                viewModel.discardDraft()
            },
        )
    }

    // The host owns any scrolling (this file's header) — the route wraps
    // `modifier` in its own `verticalScroll`; the inline hosts must not.
    // 8dp, not 12: with the detail rows behind a disclosure the blocks that
    // remain are short, and the wider gap read as air rather than as
    // separation on the Fold's 443dp cover display. Both are steps on the
    // design system's spacing scale.
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        val loadedRecord = (state as? ItemDetailState.Loaded)?.record
        // The header: the title (the draft's, so an edit shows where it was
        // made) and the X. The row itself is the wide door out — tapping it
        // closes through the same guarded path the X does.
        //
        // **The title is its own edit affordance** (operator decision
        // 2026-08-20). It used to carry a pencil beside it; the pane now
        // draws no pencil anywhere, and every editable thing here is opened
        // by tapping the thing itself. Two gestures share this row, which is
        // only safe because each consumes its own: the title's `clickable`
        // is the innermost node under a tap on the words, so it never falls
        // through to the row's close, and `IconButton` does the same for the
        // X. The row is also clickable only while *not* editing, so a tap
        // into the open field is not a tap on the way out.
        //
        // The word the pencil used to carry survives as `onClickLabel`, so
        // the gesture still names itself to a screen reader — the accessible
        // door does not close with the glyph.
        //
        // Editing ends on the field's IME Done — deliberately not on focus
        // loss, which fires once with `isFocused = false` before the field is
        // ever focused and would need a flag to tell the two apart.
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .then(if (editingTitle) Modifier else Modifier.clickable { requestClose() }),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val openDraft = draft
            if (titleFieldOpen && openDraft != null) {
                OutlinedTextField(
                    value = openDraft.title,
                    onValueChange = { viewModel.updateDraft(openDraft.copy(title = it)) },
                    modifier = Modifier
                        .weight(1f)
                        .focusRequester(titleFocus),
                    singleLine = true,
                    isError = !viewModel.canSave && openDraft.title.isEmpty(),
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { editingTitle = false }),
                )
            } else {
                // The web panel's title token is h3 (17px semibold
                // display) — titleMedium here, not headlineLarge: the
                // panel is a card over a board, not a screen. Before the
                // record lands there is nothing to name, and the meta line
                // below says so rather than this line guessing.
                val titleEditable = loadedRecord?.isEditable == true && openDraft != null
                Text(
                    openDraft?.title ?: loadedRecord?.title.orEmpty(),
                    modifier = Modifier
                        .weight(1f)
                        .then(
                            if (titleEditable) {
                                Modifier.clickable(onClickLabel = "Edit title") {
                                    editingTitle = true
                                }
                            } else {
                                Modifier
                            },
                        )
                        // The design system's 44dp row, which is also its
                        // minimum touch target: a line of `titleMedium` is
                        // shorter than that, so the target is grown to it
                        // rather than left at the height of the words.
                        .heightIn(min = 44.dp)
                        .wrapContentHeight(Alignment.CenterVertically),
                    style = MaterialTheme.typography.titleMedium,
                )
            }
            // The visible door out; the header tap is the wide one. One ×
            // whatever the state — a loading or unsynced panel must still
            // be closable.
            IconButton(onClick = { requestClose() }) {
                Icon(
                    painterResource(R.drawable.ic_x),
                    contentDescription = closeLabel,
                    modifier = Modifier.size(18.dp),
                )
            }
        }

        // `HB-<seq>` with the project riding beside it — `ItemPanel.tsx`'s
        // own `.hb-meta` line, "ITEM DETAIL" while the seq hasn't synced
        // yet, never blank. The project id stands in for an unsynced name:
        // the name is unsynced, not the project.
        val meta = buildList {
            add(loadedRecord?.seq?.let { "HB-$it" } ?: "ITEM DETAIL")
            (loadedRecord?.projectName ?: loadedRecord?.projectId)?.let { add(it) }
        }
        Text(
            meta.joinToString(" · "),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )

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
            is ItemDetailState.Loaded -> draft?.let { openDraft ->
                DetailBody(
                    itemId = itemId,
                    record = current.record,
                    draft = openDraft,
                    mode = mode,
                    dark = dark,
                    formMeta = viewModel.formMeta,
                    // Plain getters over the draft's `StateFlow.value`, so
                    // reading them subscribes to nothing. They are fresh
                    // anyway — and only because `draft` above is collected
                    // as state, so every draft change recomposes this call
                    // and recomputes them. Load-bearing: pass the draft
                    // down without collecting it and these two go stale.
                    problems = viewModel.metaProblems,
                    canSave = viewModel.canSave,
                    onDraftChange = viewModel::updateDraft,
                    onSubmit = {
                        scope.launch {
                            val landed = when (mode) {
                                ItemDetailPanelMode.SAVE ->
                                    viewModel.save(itemId, System.currentTimeMillis())
                                ItemDetailPanelMode.PROMOTE ->
                                    viewModel.promote(itemId, System.currentTimeMillis())
                            }
                            onMutated()
                            if (landed) onSubmitted()
                        }
                    },
                    onAct = { action ->
                        scope.launch {
                            viewModel.act(itemId, action, System.currentTimeMillis())
                            onMutated()
                        }
                    },
                    onComplete = {
                        scope.launch {
                            val landed = viewModel.act(itemId, "complete", System.currentTimeMillis())
                            onMutated()
                            if (landed) onSubmitted()
                        }
                    },
                    onAck = { alertId ->
                        scope.launch {
                            viewModel.ack(itemId, alertId, System.currentTimeMillis())
                            onMutated()
                        }
                    },
                    onGrill = { onGrill(itemId) },
                    hasGrillDraft = hasGrillDraft,
                    microtaskRun = microtaskRun,
                    // The current backend selection resolves to a
                    // `model` INSIDE `MicrotaskViewModel.run` (off
                    // `BackendPreference`, #274's Settings picker)
                    // — never a literal here.
                    onMicrotaskRun = { replace, grain ->
                        microtaskViewModel.run(itemId, replace, grain)
                    },
                    declinedFallbackLabel = microtaskDeclinedFallbackLabel,
                    onSwitchAndRetry = { microtaskViewModel.switchAndRetry() },
                )
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
private fun DetailBody(
    itemId: String,
    record: ItemDetailRecord,
    draft: ItemDraft,
    mode: ItemDetailPanelMode,
    dark: Boolean,
    formMeta: CaptureFormMeta,
    problems: MetaProblems?,
    canSave: Boolean,
    onDraftChange: (ItemDraft) -> Unit,
    onSubmit: () -> Unit,
    onAct: (String) -> Unit,
    onComplete: () -> Unit,
    onAck: (String) -> Unit,
    onGrill: () -> Unit,
    hasGrillDraft: Boolean,
    microtaskRun: MobileSkillRunState,
    onMicrotaskRun: (replace: Boolean, grain: Long?) -> Unit,
    declinedFallbackLabel: String?,
    onSwitchAndRetry: () -> Unit,
) {
    // One treatment per stage word (#557) — `ui/StageBadge.kt`, never a raw
    // `stage.uppercase()`. Under the header, where Triage's editor put it.
    StageBadge(stage = record.stage, dark = dark)

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

    // Everything under the axes line, behind one disclosure (operator
    // decision 2026-08-20): the axes are what the pane is read *for* — the
    // ranker's own inputs, and what a glance is after — while the notes, the
    // context and the two dates are reference material. Three condensed rows
    // at rest were most of the pane's height on the cover display.
    //
    // The chevron is `CaptureSheet`'s "More details" idiom — same glyph,
    // same half-turn, same two words, disclosing very nearly the same field
    // set, so the gesture is learned once. It is also the pane's ONLY
    // chevron, which is what keeps it from reading as the pencil it
    // replaced: a chevron means "there is more below", and a tap on a row
    // means "edit this".
    //
    // **It rides the axes row rather than sitting on a line of its own**
    // (operator decision, after seeing it on the device): centred under the
    // axes it cost a 64dp band — 8dp of panel gap, a 48dp touch target
    // around a 24dp glyph, 8dp again — which between two open editors was
    // the most conspicuous whitespace in the pane. In the row's trailing
    // slot it costs nothing at all, because that row is already 48dp tall.
    // It stays anchored to the axes row when the axes editor is open, rather
    // than following the last thing rendered: a control that moves with the
    // content above it is not a control anyone can aim at twice.
    //
    // Open by default on the promoting host, for `DetailSection`'s own
    // reason: filling these in is what the Triage queue is *for*, and fields
    // that open editable behind a shut disclosure would be invisible work.
    var detailsOverride by rememberSaveable(itemId) { mutableStateOf<Boolean?>(null) }
    val detailsOpen = detailsOverride ?: (mode == ItemDetailPanelMode.PROMOTE)

    // The three axes the system computes with, then — behind one
    // disclosure — the words a human wrote, the context, and the two dates.
    // Each section reads condensed and swaps its shared `ui/forms` editor in
    // when its line is tapped; the values live in the draft either way, and
    // one submit sends them.
    DetailSection(
        itemId = itemId,
        label = "SIZE · ENERGY · PRIORITY",
        isSet = record.size != null || record.energy != null,
        mode = mode,
        editable = record.isEditable,
        condensed = {
            // Size and energy are drawn as well as written (#558,
            // ADR-0024) — glyph beside word, one ramp colour over both —
            // and this surface has the room the ADR requires, so it is the
            // ONE place the unset ghost renders (position 0 beside an em
            // dash): `size-unset` and `size-deep` are the same three rings
            // told apart by opacity alone, which is why nothing word-free
            // ever draws a ghost.
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
                itemVerticalAlignment = Alignment.CenterVertically,
            ) {
                // The ramp position is the option's index in the
                // core-supplied list, so the seam's own order decides it
                // and no vocabulary literal lives in this file.
                val sizePos = levelPosition(formMeta.sizes.map { it.value }, record.size)
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
                val energyPos = levelPosition(formMeta.energies.map { it.value }, record.energy)
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
                val rest = buildList {
                    add("PRIORITY:${record.priority}")
                    // Read-only on this surface: the delegation axis is set
                    // and cleared deliberately elsewhere, and `ItemEdit`
                    // carries no field for it.
                    if (record.agent) add("AGENT")
                }
                Text(
                    rest.joinToString(" · "),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        },
        trailing = {
            IconButton(onClick = { detailsOverride = !detailsOpen }) {
                Icon(
                    painterResource(R.drawable.ic_chevron_down),
                    contentDescription = if (detailsOpen) "Fewer details" else "More details",
                    modifier = Modifier.rotate(if (detailsOpen) 180f else 0f),
                )
            }
        },
    ) {
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
        PriorityRow(
            selected = draft.priority,
            onSelect = { onDraftChange(draft.copy(priority = it)) },
        )
    }

    if (detailsOpen) {
        // 4dp between the three, not the panel's own gap: they are one block
        // of reference material rather than three peers of the act row.
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            DetailSection(
                itemId = itemId,
                label = "NOTES",
                isSet = record.description != null,
                mode = mode,
                editable = record.isEditable,
                condensed = {
                    record.description?.let {
                        Text(it, style = MaterialTheme.typography.bodyLarge)
                    } ?: GhostValue("NOTES")
                },
            ) {
                OutlinedTextField(
                    value = draft.description,
                    onValueChange = { onDraftChange(draft.copy(description = it)) },
                    label = { Text("Notes") },
                    modifier = Modifier.fillMaxWidth(),
                )
            }

            DetailSection(
                itemId = itemId,
                label = "CONTEXT",
                isSet = record.context != null,
                mode = mode,
                editable = record.isEditable,
                condensed = {
                    record.context?.let {
                        Text(
                            "CONTEXT:${it.uppercase()}",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    } ?: GhostValue("CONTEXT")
                },
            ) {
                // Free text over a suggestion list, not a picker: the set of places
                // a person works is theirs (CONTEXT.md's Context — an open
                // vocabulary), and the suggestions come from the seam.
                ContextField(
                    value = draft.context,
                    onValueChange = { onDraftChange(draft.copy(context = it)) },
                    suggestions = formMeta.suggestedContexts,
                )
            }

            DetailSection(
                itemId = itemId,
                label = "DATES",
                isSet = record.deadline != null || record.scheduledDate != null,
                mode = mode,
                editable = record.isEditable,
                condensed = {
                    Text(
                        listOf(
                            "DUE:${record.deadline ?: "—"}",
                            "SCHEDULED:${record.scheduledDate ?: "—"}",
                        ).joinToString(" · "),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                },
            ) {
                // The two free-text dates are the only fields that can be
                // malformed — everything else is a closed vocabulary offered as
                // choices, or the title. The problem strings are the core's,
                // shared with the web's capture box and triage form, so a bad date
                // is refused with the same words everywhere instead of being sent
                // for the authority to 400.
                CaptureDateField(
                    label = "Deadline",
                    value = draft.deadline,
                    error = problems?.deadline,
                    onValueChange = { onDraftChange(draft.copy(deadline = it)) },
                )
                CaptureDateField(
                    label = "Scheduled date",
                    value = draft.scheduledDate,
                    error = problems?.scheduledDate,
                    onValueChange = { onDraftChange(draft.copy(scheduledDate = it)) },
                )
            }
        }
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

    // #576: "Start / Mark blocked / Cancel" is wider than a phone, and a
    // plain `Row` left `Cancel` a column of letters rather than a button —
    // hence `ChoiceRow`. This row genuinely cannot fit three-plus buttons on
    // a phone, so wrapping is the right answer for it and it keeps
    // `ChoiceRow`. `complete` is filtered out: the green check below is that
    // gesture, and drawing both would offer it twice.
    ChoiceRow {
        for (action in record.availableActions.filter { it != "complete" }) {
            OutlinedButton(onClick = { onAct(action) }) {
                Text(ACTION_LABEL[action] ?: action)
            }
        }
    }

    if (record.isEditable) {
        ChoiceRow {
            // Live (#539): `itemCanGrill` is the seam's own rule, the same
            // one `TriageItemRecord.canGrill` reads per row. `isEditable`
            // is gated alongside it, and cannot be folded into
            // `itemCanGrill(record.stage)` — `stage` alone cannot tell a
            // cancelled item from a live one: `Core::act`'s cancel sets
            // `archivedAt`, never `stage`, so a cancelled Ready/In Progress
            // item still carries a `canGrill`-eligible stage. Without the
            // enclosing check, that archived row would offer a live "Grill
            // me" whose Confirm could still enqueue a `CompleteGrill` on
            // history (the same recall rule gates every editable row here).
            if (itemCanGrill(record.stage)) {
                OutlinedButton(onClick = onGrill) {
                    Text(itemGrillButtonLabel(hasGrillDraft))
                }
            }
            // One submit for the whole draft, whatever section it was typed
            // in. Its word and its destination are the mode's only job —
            // see [ItemDetailPanelMode].
            //
            // **This row must never wrap** (operator decision 2026-08-20),
            // and the word is what buys that rather than the layout: the
            // promoting submit read "Promote to ready", which with a
            // `Resume grill` beside it does not share a line on a phone.
            // `ChoiceRow` would then drop the submit to a second line — not
            // the letter-column defect #576 fixed, but the pane's two most
            // important controls stacking and moving as the Grill label
            // changes under them. `Promote` is the same domain word
            // (CONTEXT.md's Promotion) short enough to sit beside the widest
            // Grill label at the narrowest width the app ships to, which is
            // what `ItemDetailSubmitRowTest` measures. The Grill label
            // itself is not ours to shorten — it is the core's, shared
            // verbatim with the web.
            Button(onClick = onSubmit, enabled = canSave) {
                Text(
                    when (mode) {
                        ItemDetailPanelMode.SAVE -> "Save"
                        ItemDetailPanelMode.PROMOTE -> "Promote"
                    },
                )
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
            itemId = itemId,
            affordance = affordance,
            run = microtaskRun,
            onRun = onMicrotaskRun,
            declinedFallbackLabel = declinedFallbackLabel,
            onSwitchAndRetry = onSwitchAndRetry,
        )
    }

    // Bottom-right, on a line of its own rather than sharing the
    // ChoiceRow's: that row wraps at narrow widths (#576), so anything
    // beside it moves when the buttons do. Gated on the core's
    // `canMarkDone` — the wider rule that answers for Triage and Grilling,
    // where `availableActions` is empty.
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.End,
    ) {
        if (record.canMarkDone) {
            IconButton(onClick = onComplete) {
                Icon(
                    painterResource(R.drawable.ic_check),
                    contentDescription = "Mark \"${record.title}\" done",
                    modifier = Modifier.size(18.dp),
                    // `NowRow`'s own mark-done green, the same token pair
                    // and the same documented exception to "icons never
                    // carry colour independently of their label"
                    // (`NowRow.kt`'s note on it).
                    tint = if (dark) StatusDoneFgDark else Moss600,
                )
            }
        }
    }
}

/** One field group: its condensed line, or its shared `ui/forms` editor in
 * place of that line — and **the line is the control**. Tapping anywhere on
 * it opens the editor, tapping the label above the open editor shuts it
 * again.
 *
 * There is no pencil (operator decision 2026-08-20). It was a 48dp
 * `IconButton` per section, four of them stacked, and it read wrong in both
 * states: it did not say "editable" any louder than the em-dash ghost
 * beside it already does, and re-tapping it to *collapse* a section made a
 * pencil mean "done". The row-as-control costs no vertical space of its own
 * and is the same idiom `NowScreen`'s `ColumnHeader` uses for its columns.
 *
 * What a glyph gave for free and this has to pay for deliberately is the
 * gesture's *name*: `onClickLabel` carries the words the pencil's
 * `contentDescription` did, so the section still announces "Edit NOTES" /
 * "Done editing NOTES" to a screen reader. The row also grows to the design
 * system's 44dp minimum touch target, which a line of `labelSmall` is well
 * under.
 *
 * The open/shut flag starts as `null` and only becomes a real value when
 * the human taps the row, so until then the section follows the data:
 * a field the item has no value for opens editable on the
 * [ItemDetailPanelMode.PROMOTE] host — filling those in is what that queue
 * is for — and rests as a ghost everywhere else. [isSet] is read off the
 * *record*, never the draft, or typing the first character into a
 * pre-opened section would flip the default and collapse the field
 * mid-word.
 *
 * Keyed on the item id for the reason this file's header gives: under a
 * constant LazyColumn key the composable survives a selection change, so
 * an unkeyed flag would carry one item's opened sections onto the next. */
@Composable
private fun DetailSection(
    itemId: String,
    label: String,
    isSet: Boolean,
    mode: ItemDetailPanelMode,
    editable: Boolean,
    condensed: @Composable () -> Unit,
    /** A control riding this section's own row, at its trailing edge —
     * used by exactly one caller, for the pane's disclosure chevron
     * (operator decision 2026-08-20: the arrow sits in line with what is
     * above it, not on a line of its own, where its touch target cost a
     * 64dp band of whitespace).
     *
     * Two gestures then share one row, which is safe for the reason the
     * header's already is: an `IconButton` consumes its own tap, so it
     * never falls through to the row's toggle underneath. */
    trailing: (@Composable () -> Unit)? = null,
    editor: @Composable () -> Unit,
) {
    var openOverride by rememberSaveable(itemId, label) { mutableStateOf<Boolean?>(null) }
    val open = editable &&
        (openOverride ?: (mode == ItemDetailPanelMode.PROMOTE && !isSet))

    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .then(
                    if (editable) {
                        Modifier.clickable(
                            onClickLabel = if (open) "Done editing $label" else "Edit $label",
                        ) { openOverride = !open }
                    } else {
                        Modifier
                    },
                )
                .heightIn(min = 44.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(modifier = Modifier.weight(1f)) {
                if (open) {
                    Text(
                        label,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                } else {
                    condensed()
                }
            }
            trailing?.invoke()
        }
        if (open) editor()
    }
}

/** A field the item has no value for, on a surface that is mostly for
 * reading: the field's name and an em dash, in the mono meta style. Says
 * "the system holds nothing here" rather than pretending the field does not
 * exist. */
@Composable
private fun GhostValue(label: String) {
    Text(
        "$label:—",
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** The microtask affordance's own render — narrates as it streams, and a
 * decline is shown verbatim (#539's own AC), never paraphrased: #307 made
 * the seam's decline prose-only, with no reason code, precisely so nothing
 * string-matches it, here as on the web. */
@Composable
private fun MicrotaskSection(
    itemId: String,
    affordance: MobileMicrotaskAffordance,
    run: MobileSkillRunState,
    onRun: (replace: Boolean, grain: Long?) -> Unit,
    declinedFallbackLabel: String?,
    onSwitchAndRetry: () -> Unit,
) {
    // Keyed on the item, for this file's header's reason: an unkeyed grain
    // carried one item's chosen step count onto the next item selected.
    var grain by rememberSaveable(itemId) { mutableStateOf(2L) }
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

/** The discard confirmation — the app's first dialog, and deliberately its
 * only one: every host of this panel guards its draft with THIS composable
 * rather than a dialog of its own. See this file's header for why a draft
 * earns it. */
@Composable
internal fun DiscardConfirmation(onKeep: () -> Unit, onDiscard: () -> Unit) {
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
