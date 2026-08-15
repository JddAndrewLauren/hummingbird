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
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
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
import androidx.core.content.ContextCompat
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.ui.theme.HummingbirdTheme

// M1-5's capture surface (#128/#503), the second launcher icon's
// destination: field focused with the IME up on launch with zero taps,
// submit captures and finishes, mic is a button over on-device
// `SpeechRecognizer`'s raw transcript (ADR-0022 — raw text, no parsing).
// Local-first (#128): `CaptureViewModel.submit` enqueues durably before any
// network call. Raw text only — no capture-meta surface in M1 (say so here
// too, not just in ffi-mobile's doc).
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
    val dictationFailure by viewModel.dictationFailure.collectAsState()
    val focusRequester = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current

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
                .padding(24.dp),
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
                    value = draft,
                    onValueChange = viewModel::onDraftChange,
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
            // A mic that renders and then does nothing is the failure mode
            // that ADR calls a defect rather than a nit.
            dictationFailure?.let {
                Text(
                    it.message,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }

            Button(onClick = { submit() }, enabled = viewModel.canSubmit(draft)) {
                Text("Capture")
            }
        }
    }
}
