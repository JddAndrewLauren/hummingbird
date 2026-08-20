package net.twinion.hummingbird

import android.Manifest
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
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
import net.twinion.hummingbird.ui.forms.ContextField
import net.twinion.hummingbird.ui.forms.LevelSlider
import uniffi.hummingbird_ffi_mobile.CaptureDestination

// The FAB's capture sheet — the design kit's own Android capture form
// (`ui_kits/android/AndroidScreens.jsx`, `AndroidTriage`): title field with
// the dictation mic, the Triage/Ready destination pair, the energy/size
// sliders and the context field, one submit. The light form, deliberately:
// no details disclosure — `CaptureActivity` (the launcher icon's
// full-screen surface) keeps that. The mic arrived with #611, wired through
// the extracted `speech/Dictation.kt` the Activity also uses (a mic without
// recognizer plumbing was the dead control ADR-0022 calls a defect, which
// is why the first slice shipped without one). Every vocabulary word comes
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
    val dictationFailure by viewModel.dictationFailure.collectAsState()
    val focusRequester = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current

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

    fun submit() {
        scope.launch {
            if (viewModel.submit(System.currentTimeMillis())) {
                viewModel.clearDraft()
                onCaptured()
                onDismiss()
            }
        }
    }

    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 24.dp)
                .padding(bottom = 24.dp),
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
                    keyboardActions = KeyboardActions(onDone = { submit() }),
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

            // The two destinations, CaptureActivity's own segmented pair.
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(
                    selected = draft.destination == CaptureDestination.TRIAGE,
                    onClick = { viewModel.updateDraft(draft.copy(destination = CaptureDestination.TRIAGE)) },
                    label = { Text("Triage") },
                )
                FilterChip(
                    selected = draft.destination == CaptureDestination.READY,
                    onClick = { viewModel.updateDraft(draft.copy(destination = CaptureDestination.READY)) },
                    label = { Text("Ready") },
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

            Button(
                onClick = { submit() },
                enabled = viewModel.canSubmit(draft.title),
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("Capture")
            }
        }
    }
}
