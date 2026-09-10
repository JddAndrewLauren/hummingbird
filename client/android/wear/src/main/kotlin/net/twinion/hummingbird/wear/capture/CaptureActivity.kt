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
import androidx.wear.compose.material3.ConfirmationDialogDefaults
import androidx.wear.compose.material3.Icon
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.Text
import androidx.wear.input.RemoteInputIntentHelper
import androidx.wear.input.wearableExtender
import kotlinx.coroutines.launch
import net.twinion.hummingbird.brand.R
import net.twinion.hummingbird.core.WallClock
import net.twinion.hummingbird.ui.theme.StatusDoneBgDark
import net.twinion.hummingbird.ui.theme.StatusDoneFgDark
import net.twinion.hummingbird.wear.ui.theme.WearTheme
import uniffi.hummingbird_ffi_mobile.CaptureDestination

// Capture on the wrist (ADR-0039, redrawn by the 2026-09-10 design
// handoff): this activity opens the system's own input chooser at once —
// voice first, with the keyboard and the rest behind it — asks the core's
// gate whether what came back is a title, draws `DestinationScreen` over it
// (the web capture box's three squares: Triage, Mint action, Mint for
// today), hands the tapped destination to `CaptureViewModel.submit`, shows a
// confirmation naming where it landed for about a second and a half, and
// finishes. There is no hummingbird-owned microphone and no capture form:
// the chooser IS the text UI, which is also why ADR-0022 needs no amendment
// (its decision 4 already places OS text entry, dictation included, outside
// the local-only guarantee). A cancelled chooser finishes silently — the
// user changed their mind, and there is nothing to say about it. Exported,
// on its own task and out of Recents, because the capture tile launches it
// directly and a half-finished capture must never be what "the app" reopens
// to.
class CaptureActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { WearTheme { CaptureFlow(onDone = ::finish) } }
    }
}

/** What the chooser's answer became. `null` while the chooser is open or
 * a destination is still being chosen; [Captured] carries the line the
 * confirmation prints under "Captured". */
private sealed interface CaptureOutcome {
    data class Captured(val landed: String) : CaptureOutcome
    data object Refused : CaptureOutcome
}

@Composable
private fun CaptureFlow(onDone: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val viewModel = remember { CaptureViewModel.create(context) }
    // The chooser's line, once the core has agreed it is a title, until a
    // destination is tapped — the one piece of state between the two.
    var pendingText by remember { mutableStateOf<String?>(null) }
    var outcome by remember { mutableStateOf<CaptureOutcome?>(null) }

    val chooser = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        val text = result.data?.let { spokenText(it) }
        when {
            result.resultCode != Activity.RESULT_OK || text == null -> onDone()
            // The core's gate, before any choice is offered: a line it would
            // refuse gets its refusal now, not after a tap that could not
            // matter.
            !viewModel.canSubmit(text) -> outcome = CaptureOutcome.Refused
            else -> pendingText = text
        }
    }
    LaunchedEffect(Unit) { chooser.launch(captureInputIntent()) }

    fun land(destination: CaptureDestination, deadline: String) {
        val text = pendingText ?: return
        scope.launch {
            outcome = if (viewModel.submit(text, destination, deadline, System.currentTimeMillis())) {
                CaptureOutcome.Captured(landedLine(destination, deadline))
            } else {
                CaptureOutcome.Refused
            }
        }
    }

    val transcript = pendingText
    if (transcript != null && outcome == null) {
        DestinationScreen(
            transcript = transcript,
            onTriage = { land(CaptureDestination.TRIAGE, "") },
            onMint = { land(CaptureDestination.READY, "") },
            onToday = { land(CaptureDestination.READY, WallClock.todayDeadline(System.currentTimeMillis())) },
        )
    }

    val captured = outcome as? CaptureOutcome.Captured
    ConfirmationDialog(
        visible = captured != null,
        onDismissRequest = onDone,
        text = {
            Text("Captured")
            Text(
                captured?.landed ?: "",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        },
        colors = ConfirmationDialogDefaults.colors().copy(
            iconColor = StatusDoneFgDark,
            iconContainerColor = StatusDoneBgDark,
        ),
        durationMillis = CONFIRMATION_MS,
    ) {
        Icon(painterResource(R.drawable.ic_check), contentDescription = null)
    }
    ConfirmationDialog(
        visible = outcome == CaptureOutcome.Refused,
        onDismissRequest = onDone,
        text = { Text("Nothing to capture") },
        durationMillis = CONFIRMATION_MS,
    ) {
        Icon(painterResource(R.drawable.ic_x), contentDescription = null)
    }
}

/** How long either confirmation stays before the activity finishes: long
 * enough to read two short lines, short enough that the wrist is free
 * again (the handoff's "~1.6s then finish"). */
private const val CONFIRMATION_MS = 1_600L

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
