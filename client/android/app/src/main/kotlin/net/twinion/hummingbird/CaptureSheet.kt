package net.twinion.hummingbird

import android.Manifest
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
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
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.speech.DictationFailure
import net.twinion.hummingbird.ui.LevelGlyphFamily
import net.twinion.hummingbird.ui.forms.CaptureDateField
import net.twinion.hummingbird.ui.forms.ContextField
import net.twinion.hummingbird.ui.forms.DeadlineField
import net.twinion.hummingbird.ui.forms.LevelSlider
import net.twinion.hummingbird.ui.forms.LinkField
import net.twinion.hummingbird.ui.forms.PriorityRow
import net.twinion.hummingbird.ui.forms.ProjectField
import net.twinion.hummingbird.ui.theme.Sky600
import uniffi.hummingbird_ffi_mobile.CaptureDestination

// The FAB's capture sheet — the design kit's own Android capture form
// (`ui_kits/android/AndroidScreens.jsx`, `AndroidTriage`): title field with
// the dictation mic, the energy/size sliders, the context field, the
// details disclosure (a chevron since 2026-08-20), and the Triage/Add
// submit pair — two coloured glyph squares since 2026-08-21, argued at
// the row itself. It opens at full height, titleless — both operator
// decisions of the same date, each argued at its own site below.
//
// **Full field parity with `CaptureActivity`, since operator decision
// 2026-08-20.** The two surfaces differ only in which door they are — this
// one is the in-app door (the FAB, over whatever tab the reader was on),
// the Activity is the launcher door (its own task, `finish()` on submit) —
// never in what a person can record through them. The sheet was the light
// form until round 4, and the reader who reached for a deadline here and
// had to leave for the other surface is what ended that. Both forms are
// built from the same `ui/forms/` components for the same reason: a second
// hand-copy is a second place for a rule to lapse.
//
// The mic arrived with #611, wired through the extracted
// `speech/Dictation.kt` the Activity also uses (a mic without recognizer
// plumbing was the dead control ADR-0022 calls a defect, which is why the
// first slice shipped without one). Every vocabulary word comes
// from `viewModel.formMeta`, never a literal in this file
// (`CaptureFieldSetStructuralTest` names this file).
//
// One `CaptureViewModel` shape, two stores: this sheet resolves the
// ViewModel against `MainActivity`'s store, so its draft survives a
// fold/unfold like the Activity's does — but unlike the Activity, nothing
// destroys the store on submit, which is why submission here must
// `clearDraft()` (that method's own doc).
//
// Local-first exactly as the Activity: `CaptureViewModel.submit` enqueues
// durably before any network call, so `onCaptured` can start the sync leg
// while the sheet is already closing.
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CaptureSheet(
    onDismiss: () -> Unit,
    /** Fired after a successful capture, before dismissal — `AppRoot`
     * starts a user-attributed sync cycle here so `syncTick` bumps and
     * Now/Triage show the new item without waiting for the timer leg. */
    onCaptured: () -> Unit,
    /** `DictationHost.startListening` — the host (and its `destroy()`
     * lifetime) belongs to `MainActivity`'s composition of this sheet,
     * exactly as `CaptureActivity` owns its own; `CaptureScreen`'s
     * parameter shape, kept identical on purpose. */
    startListening: ((String) -> Unit, (DictationFailure) -> Unit) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    // MainActivity's store, deliberately — see this file's header.
    val viewModel: CaptureViewModel = viewModel(factory = CaptureViewModel.factory(context))
    val draft by viewModel.draft.collectAsState()
    val projects by viewModel.projects.collectAsState()
    val dictationFailure by viewModel.dictationFailure.collectAsState()
    val submitting by viewModel.submitting.collectAsState()
    val focusRequester = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    // Shut on arrival — `CaptureActivity`'s own resting state, and its
    // reason: a form that opens to nine fields taxes the common case,
    // which is one line and Enter.
    var detailsOpen by rememberSaveable { mutableStateOf(false) }
    val metaProblems = viewModel.metaProblems

    // CaptureActivity's exact idiom: ask, and a denial is reported through
    // the same failure lane as every other way a pass can end (ADR-0022's
    // no-silent-failure rule).
    val micPermission = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        if (granted) {
            startListening(viewModel::onTranscript, viewModel::onDictationFailed)
        } else {
            viewModel.onDictationFailed(DictationFailure.NO_PERMISSION)
        }
    }

    // Zero taps to a raised keyboard — CaptureActivity's own idiom.
    LaunchedEffect(Unit) {
        focusRequester.requestFocus()
        keyboard?.show()
    }

    // The Project picker's read, `CaptureActivity`'s own once-per-screen
    // crossing. `LaunchedEffect(Unit)` re-fires on a recomposition from a
    // fresh key only, and this sheet's whole composition is disposed when
    // it closes, so a re-read per open is exactly the intent — unlike the
    // screens where #634 found `LaunchedEffect(Unit)` re-firing on an
    // Activity recreation and undoing state.
    LaunchedEffect(Unit) {
        viewModel.loadProjects()
    }

    fun submit(destination: CaptureDestination) {
        scope.launch {
            if (viewModel.submit(destination, System.currentTimeMillis())) {
                viewModel.clearDraft()
                onCaptured()
                onDismiss()
            }
        }
    }

    // **Open cold at full height** (operator decision 2026-08-20): the
    // sheet is pinned to the top of the window rather than resting at the
    // half-height a `ModalBottomSheet` reaches for. Both halves are needed
    // and neither is enough alone — `skipPartiallyExpanded` removes the
    // half-height resting state, and `fillMaxHeight()` is what makes the
    // sheet itself tall, since an expanded sheet is only as tall as its
    // content and this form's content is shorter than the window with the
    // details disclosure shut. The reader who opens this is already
    // typing; a form that starts half-height and grows under the keyboard
    // moves its own fields while they do.
    //
    // Reaching the top is what makes `contentWindowInsets` load-bearing
    // here, and it was sighted on hardware rather than reasoned about: a
    // half-height sheet never met the status bar, so this sheet paid no
    // inset and did not need to. Full height, the default
    // (`safeDrawing` less the bottom edge) still left the title field's
    // outline touching the clock — the same class of dead band #614 shipped
    // in the other direction. `statusBars` is asked for by name, and only
    // that: `safeDrawing`'s IME component does not reach this sheet's
    // window at all (the scrolling column below has the measurements), so
    // naming the one inset that does is the honest form of the request.
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        modifier = Modifier.fillMaxHeight(),
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        contentWindowInsets = { WindowInsets.statusBars },
    ) {
        // **One scrolling column, submit row last — not the Activity's
        // pinned-footer split, and measured on hardware 2026-08-20 rather
        // than assumed.** Two attempts at pinning the row above the
        // keyboard failed for one underlying reason: the IME inset does not
        // reach this sheet's window. `imePadding()` was a no-op here, and
        // so was leaning on `ModalBottomSheet`'s own `contentWindowInsets`
        // (`safeDrawing`, which nominally includes the IME) — with the
        // keyboard up the content ran underneath it either way. A
        // `weight(1f, fill = false)` field column broke it a second way:
        // a `verticalScroll` child's desired height is its whole content,
        // so with `fill = false` it claimed every available pixel and left
        // the row none.
        //
        // So this sheet does what a bottom sheet does — it scrolls, the
        // focused field is brought into view by the text field itself, and
        // the submit row is the last thing in that scroll, reached by the
        // same gesture as the last field. `CaptureActivity` keeps the
        // pinned footer: it is a real window with `adjustResize`, where the
        // inset is real and pinning works.
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp)
                .padding(bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            // No screen title (operator decision 2026-08-20). The
            // `CaptureActivity` keeps its headline because it is a launcher
            // destination arriving over whatever was on screen before; this
            // sheet is a panel the reader just opened with a FAB labelled
            // "Capture", and the focused field's own placeholder asks the
            // question. A heading here spends the top of a full-height
            // sheet restating the gesture that opened it.
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                OutlinedTextField(
                    value = draft.title,
                    onValueChange = { viewModel.updateDraft(draft.copy(title = it)) },
                    modifier = Modifier
                        .weight(1f)
                        .focusRequester(focusRequester),
                    placeholder = { Text("What's on your mind?") },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                    // Enter captures to Triage, the funnel's own default —
                    // `CaptureActivity`'s identical choice.
                    keyboardActions = KeyboardActions(onDone = { submit(CaptureDestination.TRIAGE) }),
                )
                IconButton(
                    onClick = {
                        viewModel.onDictationStarted()
                        micPermission.launch(Manifest.permission.RECORD_AUDIO)
                    },
                ) {
                    Icon(painterResource(R.drawable.ic_mic), contentDescription = "Dictate")
                }
            }

            // ADR-0022: a dictation pass that ends without text says so.
            dictationFailure?.let {
                Text(
                    it.message,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                )
            }

            // Optional at capture time — unset is a legitimate resting
            // state, because deciding is mint-time work (the design kit's
            // own words on this form).
            LevelSlider(
                label = "Energy",
                glyphFamily = LevelGlyphFamily.ENERGY,
                options = viewModel.formMeta.energies,
                selected = draft.energy.ifEmpty { null },
                onSelect = { viewModel.updateDraft(draft.copy(energy = it.orEmpty())) },
            )
            LevelSlider(
                label = "Size",
                glyphFamily = LevelGlyphFamily.SIZE,
                options = viewModel.formMeta.sizes,
                selected = draft.size.ifEmpty { null },
                onSelect = { viewModel.updateDraft(draft.copy(size = it.orEmpty())) },
            )
            // Description is the one mint field that stands open, above
            // Context (operator decision 2026-08-21): a capture that needs
            // a sentence of its own needs it while the words are still in
            // the head, and a field reachable only behind a disclosure is
            // one the hand does not reach for. The rest of the mint set
            // stays behind the chevron.
            OutlinedTextField(
                value = draft.description,
                onValueChange = { viewModel.updateDraft(draft.copy(description = it)) },
                label = { Text("Description") },
                modifier = Modifier.fillMaxWidth(),
            )
            // Everything else a mint would ask, behind one disclosure — the
            // web capture box's own "More details", and `CaptureActivity`'s
            // same field set in the same order.
            // A chevron, not the words (operator decision 2026-08-20) —
            // `ic_chevron_down`, rotated a half-turn when the fields are
            // out, which is `NowScreen`'s `ColumnHeader` idiom and the
            // design system's "Unicode as icons: never" rule. The words it
            // replaces survive as the icon's `contentDescription`, so the
            // control still names itself to a screen reader.
            //
            // It rides at the right-hand end of the Context row rather than
            // centred on a line of its own (operator decision 2026-08-21),
            // which is where the web capture box already keeps it
            // (`.hb-capture-details-toggle` in `shell/responsive.css`) — a
            // row carrying nothing but one chevron spent a whole line of
            // height on it. `CenterVertically` levels it with the field's
            // box rather than its floating label.
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                ContextField(
                    value = draft.context,
                    onValueChange = { viewModel.updateDraft(draft.copy(context = it)) },
                    suggestions = viewModel.formMeta.suggestedContexts,
                    modifier = Modifier.weight(1f),
                )
                IconButton(onClick = { detailsOpen = !detailsOpen }) {
                    Icon(
                        painterResource(R.drawable.ic_chevron_down),
                        contentDescription = if (detailsOpen) "Fewer details" else "More details",
                        modifier = Modifier.rotate(if (detailsOpen) 180f else 0f),
                    )
                }
            }
            if (detailsOpen) {
                Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    ProjectField(
                        projects = projects,
                        selectedId = draft.projectId,
                        onSelect = { viewModel.updateDraft(draft.copy(projectId = it)) },
                    )
                    PriorityRow(
                        selected = draft.priority,
                        onSelect = { viewModel.updateDraft(draft.copy(priority = it)) },
                    )
                    // The two dates share a line (operator decision
                    // 2026-08-20): they are read as a pair — when it is due
                    // against when it is planned — and stacking them spent
                    // two rows of a disclosure that already holds five
                    // fields. `weight(1f)` each, `Top`-aligned so a refusal
                    // under one does not shift the other.
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalAlignment = Alignment.Top,
                    ) {
                        DeadlineField(
                            value = draft.deadline,
                            error = metaProblems.deadline,
                            onValueChange = { viewModel.updateDraft(draft.copy(deadline = it)) },
                            modifier = Modifier.weight(1f),
                        )
                        CaptureDateField(
                            label = "Scheduled date",
                            value = draft.scheduledDate,
                            error = metaProblems.scheduledDate,
                            onValueChange = { viewModel.updateDraft(draft.copy(scheduledDate = it)) },
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
            }
            // The item's one Link (#782) — `CaptureActivity`'s same field
            // in the same place, below the disclosure and outside it.
            LinkField(
                url = draft.linkUrl,
                label = draft.linkLabel,
                onUrlChange = { viewModel.updateDraft(draft.copy(linkUrl = it)) },
                onLabelChange = { viewModel.updateDraft(draft.copy(linkLabel = it)) },
                initiallyOpen = draft.linkOpen,
            )

            // `canSubmitDraft()`, not `canSubmit(draft.title)`: the two
            // free-text dates are editable on this surface now, so the
            // title rule alone would let a malformed deadline through to
            // the authority's dead-letter journal — the exact refusal
            // [CaptureViewModel.canSubmitDraft] exists for. Both buttons
            // also read the in-flight flag: two doors to one `captureFn`
            // is two ways to mint the same words twice.
            //
            // Both halves are solid coloured squares carrying a glyph and
            // no word (operator decision 2026-08-21, the same change as the
            // web's): triage's own blue with the inbox, brand orange with
            // the plus. The colour and the glyph are the label, so each
            // button's `contentDescription` is the only place its gesture
            // is named. `Sky600` is named directly rather than taken from
            // the scheme because `tertiary` is only the light scheme's blue
            // and one fill has to carry white content in both themes.
            val canSubmit = viewModel.canSubmitDraft() && !submitting
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Button(
                    onClick = { submit(CaptureDestination.TRIAGE) },
                    enabled = canSubmit,
                    colors = ButtonDefaults.buttonColors(
                        containerColor = Sky600,
                        contentColor = Color.White,
                    ),
                    modifier = Modifier.weight(1f),
                ) {
                    Icon(
                        painter = painterResource(R.drawable.ic_inbox),
                        contentDescription = "Triage",
                        modifier = Modifier.size(20.dp),
                    )
                }
                Button(
                    onClick = { submit(CaptureDestination.READY) },
                    enabled = canSubmit,
                    modifier = Modifier.weight(1f),
                ) {
                    Icon(
                        painter = painterResource(R.drawable.ic_plus),
                        contentDescription = "Add",
                        modifier = Modifier.size(20.dp),
                    )
                }
            }
        }
    }
}
