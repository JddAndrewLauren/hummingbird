package net.twinion.hummingbird

import android.content.Intent
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
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
import androidx.compose.ui.text.style.TextOverflow
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
import net.twinion.hummingbird.ui.forms.DeadlineField
import net.twinion.hummingbird.ui.forms.LevelSlider
import net.twinion.hummingbird.ui.forms.LinkField
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
import uniffi.hummingbird_ffi_mobile.linkDisplayLabel
import uniffi.hummingbird_ffi_mobile.linkIsFollowable
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
// **State is keyed per item — and for the saveable kind, keying the input
// is not what does it.** Both `viewModel(...)` calls carry
// `key = "…-$itemId"` so a different item is a different ViewModel pair —
// the inline hosts swap items without navigating, and an unkeyed lookup
// would hand item B item A's draft.
//
// Composition state is the same hazard with a different remedy, and this
// file got the remedy wrong once. Both inline hosts rendered the pane as
// `item(key = "selected-item")` — a **constant** LazyColumn key — so
// selecting another item disposed this composable and recomposed it at the
// same slot, and the slot's saveable state was saved on the way out and
// offered back on the way in. Those keys name the item now; the rest of
// this note is why that alone is not what this file relies on. `rememberSaveable(itemId)` does not stop
// that: its `inputs` only decide whether `init()` is *eligible* to run, and
// the registry is consulted first under a key derived from the **position**
// in the composition, not from the inputs. A restored value therefore wins
// over `init()` even when the input changed, which is item A's state
// arriving in item B's pane. Two of those shipped and were sighted on the
// device: the title opened in edit mode on every item selected after the
// first, and the details disclosure carried its open/shut state across.
//
// So each site here says which item it belongs to in the registry key
// itself (`key = "…-$itemId"`), the same shape the ViewModels use — with
// exactly two exceptions, both deliberate:
//
//   - title-edit mode, which is a transient mode rather than content and
//     is a plain `remember`: see it at its site.
//   - `confirmingDiscard`, the discard question, which is a bare
//     `rememberSaveable { … }` with neither an input nor a key — the one
//     site in this file still resting on the positional registry key the
//     paragraph above calls unsafe. It is safe *here* because it is
//     modal, and nothing weaker would do: the flag is only ever set true
//     by a leaving gesture that does not close, and every composition in
//     which it is true renders `DiscardConfirmation`, an `AlertDialog`,
//     which takes the touches beneath it. So no other item can be
//     selected while it is true, and a positional restore therefore
//     cannot cross items — it can only bring the same pane's own
//     unanswered question back after an Activity recreation, which is
//     exactly what it is saveable for. Keying it on the item would be
//     wrong as well as unnecessary: a question on screen belongs to the
//     gesture that asked it.
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
    // **A plain `remember`, deliberately** — the only piece of state here
    // that does not survive an Activity recreation, and the only one that
    // must not survive anything at all.
    //
    // It is a mode, not content: what the operator typed lives in the
    // ViewModel's draft and shows on the title line either way, so nothing
    // is lost by reopening the pane with the field shut. What a saveable
    // flag cost instead was a trap sighted on the device — a pane closed
    // mid-title-edit came back in edit mode, and so did the *next item
    // selected*, because the registry key is positional (this file's
    // header). A per-item registry key would have fixed the second half
    // and left the first: item A's own restored `true` is still item A's
    // trap when it is next opened.
    //
    // Keyed on the item so a selection change re-runs `init` rather than
    // carrying the mode across, which is what `remember` gives for free.
    var editingTitle by remember(itemId) { mutableStateOf(false) }
    val titleFocus = remember { FocusRequester() }
    // **Keyed on whether the field is actually there, not on the flag
    // alone.** The field only renders once there is a draft to bind, so a
    // `requestFocus()` fired on the flag alone can have no target, and
    // `FocusRequester is not initialized` is a crash, not a no-op. That was
    // sighted on hardware when the flag was restorable and could compose
    // `true` while the record was still loading; the flag no longer
    // survives a recreation (see it above), but the guard is not
    // redundant — a reload can empty the draft under an open field, and
    // keying on the condition the field renders on is what fires the
    // effect in the composition that places it and in no other.
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

    // Back escalates outward, one layer per press: the keyboard (the IME
    // takes that one itself, before this handler ever sees it), then the
    // open title field, then the draft's discard question, and only then
    // the host's own "close the item". Escaping the field is what makes it
    // escapable at all: it used to end only on the IME's Done, so a person
    // who opened it by tapping the title — the pane's own edit affordance,
    // which is easy to hit while aiming to close the pane — had no way out
    // that did not commit a title. Leaving the field does not revert what
    // was typed; the draft holds it, and the discard question is still the
    // one thing that throws work away.
    //
    // Enabled only while there is something to escape. An idle Back is
    // never fought — it belongs to the host.
    BackHandler(enabled = editingTitle || viewModel.isDirty) {
        if (editingTitle) editingTitle = false else confirmingDiscard = true
    }

    if (confirmingDiscard) {
        DiscardConfirmation(
            onKeep = { confirmingDiscard = false },
            onDiscard = {
                confirmingDiscard = false
                viewModel.discardDraft()
                // The field was editing the draft that just went away —
                // leaving it open would offer the reverted title as
                // something in flight.
                editingTitle = false
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

        // The pane's meta line: the item's project, and **no `HB-<seq>`**.
        // CLAUDE.md's repo-wide rule is that an item is named to the
        // operator by its title, never by that ref — it is a client-side
        // handle onto a uuid, no route accepts it, and a person reading one
        // cannot look the item up in the app.
        //
        // **The web still prints it** (`ItemPanel.tsx`'s `.hb-meta` line,
        // `HB-${item.seq}` with an `item detail` fallback — this line was
        // its port, and dropping the ref here is the operator's decision of
        // 2026-08-20, not a parity fix). So the two clients differ on this
        // until the rule itself is settled either way, which is #661. Do
        // not "restore consistency" by putting the ref back: that decision
        // belongs to the rule, not to this file.
        //
        // The project id stands in for an unsynced name: the name is
        // unsynced, not the project.
        //
        // **No line at all when there is no project** (operator decision
        // 2026-08-20, on seeing it): with the ref gone there is nothing
        // else to put here, and the "ITEM DETAIL" that used to be the
        // never-blank floor was a placeholder for an unsynced *seq* — kept
        // after the ref left, it read as a permanent heading under the
        // title, naming the surface instead of the item. A pane whose item
        // has no project now spends no vertical space saying so, which is
        // what the whole compacting pass was for.
        val meta = loadedRecord?.projectName ?: loadedRecord?.projectId
        if (meta != null) {
            Text(
                meta,
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
    // `key` per item, not just `inputs` per item — this file's header says
    // why, and the device pass that found it watched item B's pane open
    // already disclosed because item A's had been.
    var detailsOverride by rememberSaveable(
        itemId,
        key = "details-open-$itemId",
    ) { mutableStateOf<Boolean?>(null) }
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

    // The item's one Link (#782), and it stands OUTSIDE the disclosure
    // whenever there is one: "easy to follow the link, still possible to
    // edit it" was the decision, and a link behind a chevron is neither.
    // The row is the tap that follows it — `ACTION_VIEW`, the
    // `NowPanesExpanded.kt` idiom, never an in-app WebView — and the
    // trailing control is the edit affordance, opening the shared
    // `LinkField` under it. Drawn only for an `http(s)` URL — the core's
    // `linkIsFollowable`, the same rule the web's anchor reads — because a
    // tap on anything else would hand the system a scheme it might resolve
    // to something the operator never meant; a stored value that is not
    // followable is edited from the disclosure's `LINK` row like an absent
    // one.
    //
    // Keyed on the item in the registry key, like every other saveable
    // here (this file's header).
    val linkUrl = record.linkUrl?.takeIf { linkIsFollowable(it) }
    var editingLink by rememberSaveable(
        itemId,
        key = "link-open-$itemId",
    ) { mutableStateOf(false) }
    if (linkUrl != null && record.isEditable) {
        LinkRow(
            url = linkUrl,
            label = linkDisplayLabel(linkUrl, record.linkLabel),
            editing = editingLink,
            onEdit = { editingLink = !editingLink },
        )
        if (editingLink) {
            LinkField(
                url = draft.linkUrl,
                label = draft.linkLabel,
                onUrlChange = { onDraftChange(draft.copy(linkUrl = it)) },
                onLabelChange = { onDraftChange(draft.copy(linkLabel = it)) },
                initiallyOpen = true,
            )
        }
    } else if (linkUrl != null) {
        // History: readable, followable, not editable.
        LinkRow(
            url = linkUrl,
            label = linkDisplayLabel(linkUrl, record.linkLabel),
            editing = false,
            onEdit = null,
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
                DeadlineField(
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

            // Without a link the `LINK` ghost sits here with the other
            // reference rows, and opens the same shared field; with one,
            // the row above the disclosure is the affordance and this
            // section is not drawn — one place per state, never two.
            if (record.linkUrl == null) {
                DetailSection(
                    itemId = itemId,
                    label = "LINK",
                    isSet = record.linkUrl != null,
                    mode = mode,
                    editable = record.isEditable,
                    condensed = { GhostValue("LINK") },
                ) {
                    LinkField(
                        url = draft.linkUrl,
                        label = draft.linkLabel,
                        onUrlChange = { onDraftChange(draft.copy(linkUrl = it)) },
                        onLabelChange = { onDraftChange(draft.copy(linkLabel = it)) },
                        initiallyOpen = true,
                    )
                }
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

    // #539's microtask affordance: an *applied result*, not a re-derived
    // one — `record.microtaskAffordance` is `null` for a non-editable
    // (archived) item and `Break`/`Rewrite` otherwise, decided by
    // `hummingbird_core::decisions::skills::microtask_affordance`. Nothing
    // here — neither this narration nor the button in the action row below
    // — offers any eligibility logic of its own.
    if (record.microtaskAffordance != null) {
        MicrotaskNarration(
            run = microtaskRun,
            declinedFallbackLabel = declinedFallbackLabel,
            onSwitchAndRetry = onSwitchAndRetry,
        )
    }

    // The pane's one action row (operator decision 2026-08-20): the grill,
    // the microtask affordance, the submit and the mark-done check share a
    // line, where they used to occupy three vertical slices — a `ChoiceRow`
    // of two buttons, the microtask section's own button, and a row holding
    // nothing but the check.
    //
    // **Labels are what buys the line, and there is not enough of it for
    // four.** The narrowest host is the notification route, which pays
    // `.padding(24.dp)` around the panel, so on a 320dp phone this row is
    // laid out in 272dp; four labelled controls measure ~325dp even with
    // the words cut to `Grill`/`Steps`, and the real labels are wider still
    // (`Resume grill` 131dp, `Rewrite 3 steps` 149dp). So the two agent
    // affordances are icon-only at 48dp each and only the submit keeps its
    // word: 48 + 48 + 105 + 48 = 249dp of 272dp (the submit measures 105dp,
    // not the 114dp this comment used to claim) — it fits, but only just,
    // which is what the cap below is for.
    // Neither label is lost, only unprinted — each rides its icon's
    // accessible name, and both are the core's own strings shared verbatim
    // with the web (`itemGrillButtonLabel`, and the affordance's own
    // words), not this surface's to shorten. `ItemDetailSubmitRowTest`
    // measures the row at 272dp and carries that arithmetic.
    //
    // The two agent affordances lead, the two writes trail, and the
    // `weight(1f)` between them is what keeps the submit and the check
    // anchored right whichever of the leading pair renders — a control that
    // moves as its neighbours appear is not one anyone can aim at twice.
    //
    // **The submit is the row's only elastic control, and it is capped.** A
    // plain `Row` measures its non-weighted children in composition order,
    // each against whatever width is left, and the mark-done check is
    // composed **last** — so the check is what this row spends when the
    // submit's label grows, and it spends it silently. Measured uncapped at
    // 272dp (`ItemDetailSubmitRowTest`), the check's touch target goes 40dp
    // at 1.6x font scale → 37 at 1.7 → 33 at 1.8 → 30 at 1.9 → 24 at 2.0 →
    // 12 at 2.2 → **0 at 2.5**: a write control gone with no sign it was
    // ever there. Which is the failure `NowScreen`'s frontier chips name
    // too ("a fixed `Row` squeezes whatever runs out of width, and the chip
    // at the trailing edge is … hidden with no sign it is there"), and it
    // is worse here, because what vanishes is a write rather than a filter.
    //
    // The fix is arithmetic, not layout: the three touch targets are the
    // floor at Material's nominal 48dp each, so the submit may have
    // 272 − 3 × 48 = **128dp** and no more, and its label goes to one
    // ellipsised line rather than growing the row. Derived from the nominal
    // rather than the 40dp the buttons actually measure, so the cap is the
    // conservative side of the real need.
    //
    // At default scale `Promote` measures 105dp — 23dp inside the cap — so
    // the row renders exactly as it did, and the test measures that rather
    // than trusting the eye. Past the cap the submit is what gives way,
    // and the check holds a whole touch target to 3.0x. The row does not
    // wrap at any scale — operator decision; `ChoiceRow` is what wrapping
    // looks like where it is wanted, one band up in the act row.
    val submitMaxWidth = 128.dp
    // The grain the `Rewrite` affordance asks for. A plain constant, not
    // state: **no surface in this build can write it** — there is no
    // stepper, picker or seam for it anywhere, and this was a
    // `rememberSaveable` whose comment described carrying "the chosen step
    // count" between items, a capability that never existed. If a control
    // for it lands it becomes saveable state again, keyed on the item per
    // this file's header, and the structural sweep over `rememberSaveable(`
    // will cover it.
    val grain = 2L
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
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
        if (record.isEditable && itemCanGrill(record.stage)) {
            IconButton(onClick = onGrill) {
                Icon(
                    painterResource(R.drawable.ic_messages_square),
                    contentDescription = itemGrillButtonLabel(hasGrillDraft),
                    modifier = Modifier.size(18.dp),
                )
            }
        }
        record.microtaskAffordance?.let { affordance ->
            IconButton(
                enabled = microtaskRun !is MobileSkillRunState.Running,
                onClick = {
                    when (affordance) {
                        MobileMicrotaskAffordance.Break -> onMicrotaskRun(false, null)
                        is MobileMicrotaskAffordance.Rewrite -> onMicrotaskRun(true, grain)
                    }
                },
            ) {
                Icon(
                    painterResource(R.drawable.ic_list_checks),
                    contentDescription = microtaskLabel(affordance),
                    modifier = Modifier.size(18.dp),
                )
            }
        }

        Spacer(Modifier.weight(1f))

        if (record.isEditable) {
            // One submit for the whole draft, whatever section it was typed
            // in. Its word and its destination are the mode's only job —
            // see [ItemDetailPanelMode]. `Promote` rather than `Promote to
            // ready` is the same domain word (CONTEXT.md's Promotion) at a
            // width this row can pay for; the long one cost 160dp.
            //
            // [submitMaxWidth] is the cap that keeps the three 48dp targets
            // out of the squeeze, and `maxLines = 1` with an ellipsis is
            // what the label does instead of growing the row when the cap
            // bites — see the note where the cap is derived.
            Button(
                onClick = onSubmit,
                enabled = canSave,
                modifier = Modifier.widthIn(max = submitMaxWidth),
            ) {
                Text(
                    when (mode) {
                        ItemDetailPanelMode.SAVE -> "Save"
                        ItemDetailPanelMode.PROMOTE -> "Promote"
                    },
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }

        // Gated on the core's `canMarkDone` — the wider rule that answers
        // for Triage and Grilling, where `availableActions` is empty.
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
    var openOverride by rememberSaveable(
        itemId,
        label,
        key = "section-open-$itemId-$label",
    ) { mutableStateOf<Boolean?>(null) }
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

/** The item's Link as a row (#782): the chain glyph, the core's display
 * label — the name, else the host — and the outward-arrow mark both clients
 * put on a tap that leaves the app. The whole row follows the link through
 * `ACTION_VIEW`; the trailing control, when the item is editable, is the
 * edit affordance. Two gestures share the row, which is safe for the reason
 * the header's already is: an `IconButton` consumes its own tap.
 *
 * Only ever drawn for an `http(s)` URL — the caller asks the core
 * (`linkIsFollowable`), which is what keeps a `javascript:` or `intent:`
 * string out of `ACTION_VIEW`; the row itself has no opinion. */
@Composable
private fun LinkRow(
    url: String,
    label: String,
    editing: Boolean,
    onEdit: (() -> Unit)?,
) {
    val context = LocalContext.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable {
                context.startActivity(
                    Intent(Intent.ACTION_VIEW, Uri.parse(url))
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                )
            }
            .heightIn(min = 44.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            painterResource(R.drawable.ic_link),
            contentDescription = null,
            modifier = Modifier.size(16.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Icon(
            painterResource(R.drawable.ic_arrow_up_right),
            contentDescription = "Opens in the browser",
            modifier = Modifier.size(13.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (onEdit != null) {
            // The ellipsis, not a pencil: the pane draws no pencil
            // (operator decision 2026-08-20), and the tapped thing here is
            // already spoken for by the link itself.
            IconButton(onClick = onEdit) {
                Icon(
                    painterResource(R.drawable.ic_ellipsis),
                    contentDescription = if (editing) "Done editing link" else "Edit link",
                    modifier = Modifier.size(18.dp),
                )
            }
        }
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

/** The words the microtask affordance offers, which since the pane's one
 * action row (2026-08-20) are spoken by its icon's accessible name rather
 * than printed on a button — the row has no width for a third label. The
 * count is the affordance's own applied number, so "Rewrite 1 step" is
 * never pluralised wrong.
 *
 * `internal` rather than private only so its pluralisation can be asserted
 * directly (`ItemDetailSubmitRowTest`) — it is the whole accessible name of
 * a control that prints no words, so "Rewrite 1 steps" is a defect nothing
 * else in this module would see, and Robolectric cannot read the string off
 * a rendered icon. Same widening, for the same reason, as
 * `DiscardConfirmation` below. Still nobody else's to call. */
internal fun microtaskLabel(affordance: MobileMicrotaskAffordance) =
    when (affordance) {
        MobileMicrotaskAffordance.Break -> "Break into steps"
        is MobileMicrotaskAffordance.Rewrite ->
            "Rewrite ${affordance.undoneCount} step" +
                if (affordance.undoneCount == 1u) "" else "s"
    }

/** What the microtask run says while it runs — narrates as it streams, and
 * a decline is shown verbatim (#539's own AC), never paraphrased: #307 made
 * the seam's decline prose-only, with no reason code, precisely so nothing
 * string-matches it, here as on the web.
 *
 * The run's *button* is not here: it sits in the pane's one action row with
 * the grill and the submit. This block is only the answer, and it renders
 * above that row — the last line of a pane is where the controls are, not
 * where a stream of narration should push them. Nothing at all before the
 * first run, since an `Idle` run has nothing to say. */
@Composable
private fun MicrotaskNarration(
    run: MobileSkillRunState,
    declinedFallbackLabel: String?,
    onSwitchAndRetry: () -> Unit,
) {
    if (run is MobileSkillRunState.Idle) return

    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
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
