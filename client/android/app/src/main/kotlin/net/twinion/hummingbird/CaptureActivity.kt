package net.twinion.hummingbird

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MenuAnchorType
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
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
import androidx.core.content.ContextCompat
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.ui.forms.CaptureDateField
import net.twinion.hummingbird.ui.forms.ContextField
import net.twinion.hummingbird.ui.forms.LevelSlider
import net.twinion.hummingbird.ui.forms.PriorityRow
import net.twinion.hummingbird.ui.theme.HummingbirdTheme
import uniffi.hummingbird_ffi_mobile.CaptureDestination
import uniffi.hummingbird_ffi_mobile.MobileProject

// M1-5's capture surface (#128/#503), the second launcher icon's
// destination: field focused with the IME up on launch with zero taps,
// submit captures and finishes, mic is a button over on-device
// `SpeechRecognizer`'s raw transcript (ADR-0022 — raw text, no parsing).
// Local-first (#128): `CaptureViewModel.submit` enqueues durably before any
// network call.
//
// M3/#529 widened this from title-only to the web capture box's whole
// field set: a destination choice (Triage/Ready), the energy/size sliders,
// the open-vocabulary context field, and a details disclosure holding
// description, project, priority, deadline and scheduled date. Dictation
// stays title-field-only (`CaptureViewModel.onTranscript`'s own doc) — say
// so here too, not just there. `LevelSlider`/`ContextField`/
// `CaptureDateField` (`ui/forms/`) are the shared components this screen
// builds and the Triage screen (#531) reuses; every vocabulary word they
// render comes from `viewModel.formMeta`
// (`uniffi.hummingbird_ffi_mobile.captureFormMeta`), never a literal typed
// into this file (ADR-0025's ban on a hand-copied vocabulary).
class CaptureActivity : ComponentActivity() {

    private var recognizer: SpeechRecognizer? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            HummingbirdTheme {
                CaptureScreen(
                    startListening = ::startListening,
                    onFinished = ::finish,
                )
            }
        }
    }

    override fun onDestroy() {
        recognizer?.destroy()
        recognizer = null
        super.onDestroy()
    }

    /** Starts one **on-device** recognition pass; `onTranscript` receives
     * the first hypothesis verbatim, and `onFailure` every other way the
     * pass can end.
     *
     * ADR-0022's prohibition is on audio leaving the device, and
     * `isRecognitionAvailable`/`createSpeechRecognizer` do not establish
     * that: they resolve the *default* recognition service, which the
     * platform documents as free to send audio to a remote server. Only the
     * `…OnDevice…` pair positively establishes local processing, so those
     * are what this asks for, and an absent on-device recognizer means the
     * feature is unavailable — never a quiet fall back to the default
     * service, which is exactly the "network-backed recognizer as an
     * error-path fallback" the ADR rejects by name.
     *
     * Every failure path ends the session and reports, per the same ADR:
     * "the prohibition explicitly includes error paths." */
    private fun startListening(
        onTranscript: (String) -> Unit,
        onFailure: (DictationFailure) -> Unit,
    ) {
        if (ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            onFailure(DictationFailure.NO_PERMISSION)
            return
        }
        if (!SpeechRecognizer.isOnDeviceRecognitionAvailable(this)) {
            onFailure(DictationFailure.UNAVAILABLE)
            return
        }
        val active = recognizer
            ?: SpeechRecognizer.createOnDeviceSpeechRecognizer(this).also { recognizer = it }
        active.setRecognitionListener(TranscriptListener(onTranscript, onFailure))
        val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
            putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
        }
        active.startListening(intent)
    }
}

/** The ways a dictation pass can end without text, each with the sentence
 * the capture screen shows for it. One enum rather than four call sites
 * composing strings, because ADR-0022 treats the *set* as the requirement:
 * unavailable, refused, mid-session error, no match — all four visible.
 * Wording follows the design README's honesty rule: say what happened and
 * what still works, apologise for nothing. */
enum class DictationFailure(val message: String) {
    NO_PERMISSION("Dictation needs microphone access. Typing still works."),
    UNAVAILABLE("This device has no on-device speech recognition. Typing still works."),
    FAILED("Dictation stopped. Typing still works."),
    NO_MATCH("No speech recognised."),
}

/** A raw-transcript-only listener (ADR-0022): the first hypothesis's text,
 * verbatim, never trimmed or parsed here — [CaptureViewModel.onTranscript]
 * is the next stop, and it does not touch the string either. A result that
 * carries no hypothesis is a failure, not a silent no-op: an empty
 * `RESULTS_RECOGNITION` is the no-match case the ADR names. */
private class TranscriptListener(
    private val onTranscript: (String) -> Unit,
    private val onFailure: (DictationFailure) -> Unit,
) : RecognitionListener {
    override fun onResults(results: Bundle) {
        val transcript = results.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()
        if (transcript == null) {
            onFailure(DictationFailure.NO_MATCH)
            return
        }
        onTranscript(transcript)
    }

    /** The recognizer has already ended the session by the time this
     * arrives; the only thing left is to say so. Nothing here retries, and
     * nothing here reaches for a second recognizer — that is the fallback
     * ADR-0022 rejects. */
    override fun onError(error: Int) {
        val failure = when (error) {
            SpeechRecognizer.ERROR_NO_MATCH,
            SpeechRecognizer.ERROR_SPEECH_TIMEOUT,
            -> DictationFailure.NO_MATCH
            SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> DictationFailure.NO_PERMISSION
            else -> DictationFailure.FAILED
        }
        onFailure(failure)
    }

    override fun onReadyForSpeech(params: Bundle?) {}
    override fun onBeginningOfSpeech() {}
    override fun onRmsChanged(rmsdB: Float) {}
    override fun onBufferReceived(buffer: ByteArray?) {}
    override fun onEndOfSpeech() {}
    override fun onPartialResults(partialResults: Bundle?) {}
    override fun onEvent(eventType: Int, params: Bundle?) {}
}

@Composable
private fun CaptureScreen(
    startListening: ((String) -> Unit, (DictationFailure) -> Unit) -> Unit,
    onFinished: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    // Activity-scoped, not composition-scoped: see CaptureViewModel.factory.
    val viewModel: CaptureViewModel = viewModel(factory = CaptureViewModel.factory(context))
    val draft by viewModel.draft.collectAsState()
    val projects by viewModel.projects.collectAsState()
    val dictationFailure by viewModel.dictationFailure.collectAsState()
    val focusRequester = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    // Shut on arrival, matching the web capture box's own resting state
    // (`CaptureBox.tsx`'s `detailsOpen`) — a form that opens to seven
    // fields taxes the common case, which is one line and Enter.
    var detailsOpen by rememberSaveable { mutableStateOf(false) }
    val metaProblems = viewModel.metaProblems

    val micPermission = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        if (granted) {
            startListening(viewModel::onTranscript, viewModel::onDictationFailed)
        } else {
            viewModel.onDictationFailed(DictationFailure.NO_PERMISSION)
        }
    }

    // Zero taps to a raised keyboard: request focus and show the IME the
    // instant this screen composes.
    LaunchedEffect(Unit) {
        focusRequester.requestFocus()
        keyboard?.show()
    }

    // The Project picker's read, once per screen (review finding on #529's
    // own PR): an opaque, hand-typed project id was a dead-letter hazard —
    // `items.project_id` is an FK — so the details disclosure offers the
    // real live list instead, the same "No project" + list `CaptureBox.tsx`
    // (`client/web/src/screens/CaptureBox.tsx:830-839`) already renders.
    LaunchedEffect(Unit) {
        viewModel.loadProjects()
    }

    fun submit() {
        scope.launch {
            if (viewModel.submit(System.currentTimeMillis())) {
                onFinished()
            }
        }
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
            // The product name is lowercase everywhere; the screen title is
            // the one exception the design system already carries (a verb,
            // not the brand).
            Text("Capture", style = MaterialTheme.typography.headlineLarge)

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

            // The two destinations the web capture box offers as Triage/Mint
            // (`CaptureBox.tsx`) — here as a two-way choice rather than two
            // submit buttons, since one "Capture" action already ends the
            // Activity. `FilterChip`'s own selected/unselected pair is the
            // native control closest to a segmented toggle.
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

            // Energy/Size (the frontier's axes) and Context (the open
            // vocabulary) — every word rendered here comes from
            // `viewModel.formMeta`, never a literal in this file.
            LevelSlider(
                label = "Energy",
                options = viewModel.formMeta.energies,
                selected = draft.energy.ifEmpty { null },
                onSelect = { viewModel.updateDraft(draft.copy(energy = it.orEmpty())) },
            )
            LevelSlider(
                label = "Size",
                options = viewModel.formMeta.sizes,
                selected = draft.size.ifEmpty { null },
                onSelect = { viewModel.updateDraft(draft.copy(size = it.orEmpty())) },
            )
            ContextField(
                value = draft.context,
                onValueChange = { viewModel.updateDraft(draft.copy(context = it)) },
                suggestions = viewModel.formMeta.suggestedContexts,
            )

            // ADR-0022: a dictation pass that ends without text says so.
            // A mic that renders and then does nothing is the failure mode
            // that ADR calls a defect rather than a nit.
            dictationFailure?.let {
                Text(
                    it.message,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                )
            }

            // Everything a mint would ask, behind one disclosure — the web
            // capture box's own "More details" (`CaptureBox.tsx`).
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

            Button(onClick = { submit() }, enabled = viewModel.canSubmitDraft()) {
                Text("Capture")
            }
        }
    }
}

/** The details disclosure's Project picker (review finding on #529's own
 * PR): a read-only `ExposedDropdownMenuBox` over the real live
 * [projects][MobileProject] list plus a leading "No project" entry — the
 * same shape `CaptureBox.tsx:830-839`'s `Select` offers on the web side.
 * An opaque, hand-typed project id would mint locally and be refused at
 * the authority the first time it did not match a real row
 * (`items.project_id` is `TEXT REFERENCES projects(id)`,
 * `server/authority/src/schema.rs:135`) — exactly the dead-letter failure
 * `CaptureViewModel.canSubmitDraft`'s own doc says the two-predicate gate
 * exists to prevent, so this field must offer only ids that already exist.
 * `readOnly = true` on the anchor field is what makes that true: nothing
 * here lets a reader type a project id at all. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ProjectField(
    projects: List<MobileProject>,
    selectedId: String,
    onSelect: (String) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    val selectedName = projects.find { it.id == selectedId }?.name ?: "No project"

    ExposedDropdownMenuBox(
        expanded = expanded,
        onExpandedChange = { expanded = it },
    ) {
        OutlinedTextField(
            value = selectedName,
            onValueChange = {},
            readOnly = true,
            label = { Text("Project") },
            modifier = Modifier
                .fillMaxWidth()
                .menuAnchor(MenuAnchorType.PrimaryNotEditable),
        )
        ExposedDropdownMenu(
            expanded = expanded,
            onDismissRequest = { expanded = false },
        ) {
            DropdownMenuItem(
                text = { Text("No project") },
                onClick = {
                    onSelect("")
                    expanded = false
                },
            )
            for (project in projects) {
                DropdownMenuItem(
                    text = { Text(project.name) },
                    onClick = {
                        onSelect(project.id)
                        expanded = false
                    },
                )
            }
        }
    }
}

