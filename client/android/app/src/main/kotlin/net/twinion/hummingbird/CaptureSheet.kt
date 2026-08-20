package net.twinion.hummingbird

import android.Manifest
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
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
import net.twinion.hummingbird.ui.forms.LevelSlider
import net.twinion.hummingbird.ui.forms.PriorityRow
import net.twinion.hummingbird.ui.forms.ProjectField
import uniffi.hummingbird_ffi_mobile.CaptureDestination

// The FAB's capture sheet — the design kit's own Android capture form
// (`ui_kits/android/AndroidScreens.jsx`, `AndroidTriage`): title field with
// the dictation mic, the energy/size sliders, the context field, the "More
// details" disclosure, and the Triage/Add submit pair.
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

    ModalBottomSheet(onDismissRequest = onDismiss) {
        // Two children, and the split is the point (`CaptureActivity`'s
        // own): the fields scroll, the submit row does not. With the
        // details disclosure open the form is taller than a sheet at a
        // raised keyboard, and before the split the buttons were the last
        // thing in the column — the only way out, scrolled off the bottom.
        // `weight(1f, fill = false)` so a one-line draft still leaves the
        // sheet at its natural height rather than stretching it full.
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .imePadding()
                .navigationBarsPadding(),
        ) {
            Column(
                modifier = Modifier
                    .weight(1f, fill = false)
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 24.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                // The screen title is the design system's one non-lowercase
                // exception (a verb, not the brand).
                Text("Capture", style = MaterialTheme.typography.headlineSmall)

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
                ContextField(
                    value = draft.context,
                    onValueChange = { viewModel.updateDraft(draft.copy(context = it)) },
                    suggestions = viewModel.formMeta.suggestedContexts,
                )

                // Everything a mint would ask, behind one disclosure — the web
                // capture box's own "More details", and `CaptureActivity`'s
                // same field set in the same order.
                TextButton(onClick = { detailsOpen = !detailsOpen }) {
                    Text(if (detailsOpen) "Fewer details" else "More details")
                }
                if (detailsOpen) {
                    Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
                        OutlinedTextField(
                            value = draft.description,
                            onValueChange = { viewModel.updateDraft(draft.copy(description = it)) },
                            label = { Text("Description") },
                            modifier = Modifier.fillMaxWidth(),
                        )
                        ProjectField(
                            projects = projects,
                            selectedId = draft.projectId,
                            onSelect = { viewModel.updateDraft(draft.copy(projectId = it)) },
                        )
                        PriorityRow(
                            selected = draft.priority,
                            onSelect = { viewModel.updateDraft(draft.copy(priority = it)) },
                        )
                        CaptureDateField(
                            label = "Deadline",
                            value = draft.deadline,
                            error = metaProblems.deadline,
                            onValueChange = { viewModel.updateDraft(draft.copy(deadline = it)) },
                        )
                        CaptureDateField(
                            label = "Scheduled date",
                            value = draft.scheduledDate,
                            error = metaProblems.scheduledDate,
                            onValueChange = { viewModel.updateDraft(draft.copy(scheduledDate = it)) },
                        )
                    }
                }
            }

            // `canSubmitDraft()`, not `canSubmit(draft.title)`: the two
            // free-text dates are editable on this surface now, so the
            // title rule alone would let a malformed deadline through to
            // the authority's dead-letter journal — the exact refusal
            // [CaptureViewModel.canSubmitDraft] exists for. Both buttons
            // also read the in-flight flag: two doors to one `captureFn`
            // is two ways to mint the same words twice.
            val canSubmit = viewModel.canSubmitDraft() && !submitting
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 24.dp)
                    .padding(top = 12.dp, bottom = 24.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                OutlinedButton(
                    onClick = { submit(CaptureDestination.TRIAGE) },
                    enabled = canSubmit,
                    modifier = Modifier.weight(1f),
                ) {
                    Text("Triage")
                }
                Button(
                    onClick = { submit(CaptureDestination.READY) },
                    enabled = canSubmit,
                    modifier = Modifier.weight(1f),
                ) {
                    Text("Add")
                }
            }
        }
    }
}
