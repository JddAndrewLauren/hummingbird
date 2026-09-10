package net.twinion.hummingbird.wear.capture

import android.app.Activity
import android.app.RemoteInput
import android.content.Intent
import android.os.Bundle
import android.view.inputmethod.EditorInfo
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.wear.compose.material3.ConfirmationDialog
import androidx.wear.compose.material3.Icon
import androidx.wear.compose.material3.Text
import androidx.wear.input.RemoteInputIntentHelper
import androidx.wear.input.wearableExtender
import kotlinx.coroutines.launch
import net.twinion.hummingbird.brand.R
import net.twinion.hummingbird.wear.ui.theme.WearTheme

// Capture on the wrist (ADR-0039): this activity opens the system's own
// input chooser at once — voice first, with the keyboard and the rest behind
// it — hands whatever comes back to `CaptureViewModel.submit`, shows a
// confirmation for about a second and a half, and finishes. There is no
// hummingbird-owned microphone and no capture form: the chooser IS the UI,
// which is also why ADR-0022 needs no amendment (its decision 4 already
// places OS text entry, dictation included, outside the local-only
// guarantee). A cancelled chooser finishes silently — the user changed
// their mind, and there is nothing to say about it. Exported, on its own
// task and out of Recents, because the capture tile launches it directly
// and a half-finished capture must never be what "the app" reopens to.
class CaptureActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { WearTheme { CaptureFlow(onDone = ::finish) } }
    }
}

/** What the chooser's answer became — `null` while it is still open. */
private enum class CaptureOutcome { CAPTURED, REFUSED }

@Composable
private fun CaptureFlow(onDone: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val viewModel = remember { CaptureViewModel.create(context) }
    var outcome by remember { mutableStateOf<CaptureOutcome?>(null) }

    val chooser = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        val text = result.data?.let { spokenText(it) }
        if (result.resultCode != Activity.RESULT_OK || text == null) {
            onDone()
        } else {
            scope.launch {
                outcome = if (viewModel.submit(text, System.currentTimeMillis())) {
                    CaptureOutcome.CAPTURED
                } else {
                    CaptureOutcome.REFUSED
                }
            }
        }
    }
    LaunchedEffect(Unit) { chooser.launch(captureInputIntent()) }

    ConfirmationDialog(
        visible = outcome == CaptureOutcome.CAPTURED,
        onDismissRequest = onDone,
        text = { Text("Captured") },
    ) {
        Icon(painterResource(R.drawable.ic_check), contentDescription = null)
    }
    ConfirmationDialog(
        visible = outcome == CaptureOutcome.REFUSED,
        onDismissRequest = onDone,
        text = { Text("Nothing to capture") },
    ) {
        Icon(painterResource(R.drawable.ic_x), contentDescription = null)
    }
}

/** The one `RemoteInput` result key. */
internal const val KEY_TEXT = "net.twinion.hummingbird.wear.capture.TEXT"

/** The system input chooser, asked for one free-form line. */
internal fun captureInputIntent(): Intent {
    val input = RemoteInput.Builder(KEY_TEXT)
        .setLabel("Capture")
        .setAllowFreeFormInput(true)
        // Wear's own extender: emoji allowed (a title may be one), and the
        // keyboard's action key asked to read Done — a capture is finished,
        // not sent to someone. (The Wear OS 7.0 emulator's Gboard still
        // showed Send and its result never reached the chooser there; the
        // emoji path through the same chooser did — README, "The watch".)
        .wearableExtender {
            setEmojisAllowed(true)
            setInputActionType(EditorInfo.IME_ACTION_DONE)
        }
        .build()
    return RemoteInputIntentHelper.createActionRemoteInputIntent().also {
        RemoteInputIntentHelper.putRemoteInputsExtra(it, listOf(input))
        RemoteInputIntentHelper.putTitleExtra(it, "Capture")
    }
}

/** The chooser's answer, or `null` when it carried none (a cancel, or an
 * empty result bundle). Passed on as given — whether it is a title is the
 * core's call. */
internal fun spokenText(data: Intent): String? =
    RemoteInput.getResultsFromIntent(data)?.getCharSequence(KEY_TEXT)?.toString()
