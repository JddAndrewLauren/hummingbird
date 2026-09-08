package net.twinion.hummingbird

import android.Manifest
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
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
import net.twinion.hummingbird.speech.DictationHost
import net.twinion.hummingbird.ui.LevelGlyphFamily
import net.twinion.hummingbird.ui.forms.CaptureDateField
import net.twinion.hummingbird.ui.forms.ContextField
import net.twinion.hummingbird.ui.forms.DeadlineField
import net.twinion.hummingbird.ui.forms.LevelSlider
import net.twinion.hummingbird.ui.forms.LinkField
import net.twinion.hummingbird.ui.forms.PriorityRow
import net.twinion.hummingbird.ui.forms.ProjectField
import net.twinion.hummingbird.ui.theme.HummingbirdTheme
import net.twinion.hummingbird.ui.theme.Sky600
import uniffi.hummingbird_ffi_mobile.CaptureDestination
import uniffi.hummingbird_ffi_mobile.parseSharePayload

// M1-5's capture surface (#128/#503), the second launcher icon's
// destination: field focused with the IME up on launch with zero taps,
// submit captures and finishes, mic is a button over on-device
// `SpeechRecognizer`'s raw transcript (ADR-0022 — raw text, no parsing).
// Local-first (#128): `CaptureViewModel.submit` enqueues durably before any
// network call.
//
// M3/#529 widened this from title-only to the web capture box's whole
// field set: the energy/size sliders, the open-vocabulary context field,
// the description box (out from behind the disclosure and above Context
// since 2026-08-21, argued at the field), and a details disclosure holding
// project, priority, deadline and scheduled date. The destination rides on the submit gesture
// — Triage and Add are two buttons, not a switch above one, and since
// 2026-08-21 two coloured glyph squares rather than two words (argued at
// the row itself) — and the button row is pinned below the scrolling
// fields so the keyboard can never push it out of reach. Dictation stays title-field-only
// (`CaptureViewModel.onTranscript`'s own doc) — say so here too, not just
// there. `LevelSlider`/`ContextField`/`CaptureDateField`/`ProjectField`
// (`ui/forms/`) are the shared components this screen builds and both the
// Triage screen (#531) and the capture sheet reuse; every vocabulary word they
// render comes from `viewModel.formMeta`
// (`uniffi.hummingbird_ffi_mobile.captureFormMeta`), never a literal typed
// into this file (ADR-0025's ban on a hand-copied vocabulary).
//
// #782 made this the share target as well: the `.ShareTarget` alias in the
// manifest routes a `text/plain` `ACTION_SEND` here, and the subject and
// text ride into `CaptureScreen`, where `parseSharePayload` — the core's
// mapping, crossed on the seam — seeds the draft once. Zero URL parsing in
// Kotlin (`ManifestAliasTest` pins that). Attended, never unattended: the
// share lands on this same form, prefilled, and the reader still presses
// submit; `finish()` after it returns to the sharing app.
class CaptureActivity : ComponentActivity() {

    // The recognizer plumbing lives in `speech/Dictation.kt` since #611 —
    // shared with the FAB's capture sheet by extraction, never by a second
    // copy. This Activity owns its host for its own lifetime.
    private var dictation: DictationHost? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val host = DictationHost(this).also { dictation = it }
        // A share's payload, or nothing: the launcher and shortcut doors
        // arrive with no action of interest and seed nothing.
        val share = if (intent?.action == Intent.ACTION_SEND) {
            SharePayload(
                subject = intent.getStringExtra(Intent.EXTRA_SUBJECT).orEmpty(),
                text = intent.getStringExtra(Intent.EXTRA_TEXT).orEmpty(),
            )
        } else {
            null
        }
        setContent {
            HummingbirdTheme {
                CaptureScreen(
                    startListening = host::startListening,
                    onFinished = ::finish,
                    share = share,
                )
            }
        }
    }

    override fun onDestroy() {
        dictation?.destroy()
        dictation = null
        super.onDestroy()
    }
}

/** What an `ACTION_SEND` share handed this Activity, verbatim — the two
 * extras and nothing derived from them. */
private data class SharePayload(val subject: String, val text: String)

@Composable
private fun CaptureScreen(
    startListening: ((String) -> Unit, (DictationFailure) -> Unit) -> Unit,
    onFinished: () -> Unit,
    share: SharePayload? = null,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    // Activity-scoped, not composition-scoped: see CaptureViewModel.factory.
    val viewModel: CaptureViewModel = viewModel(factory = CaptureViewModel.factory(context))
    val draft by viewModel.draft.collectAsState()
    val projects by viewModel.projects.collectAsState()
    val dictationFailure by viewModel.dictationFailure.collectAsState()
    val submitting by viewModel.submitting.collectAsState()
    val focusRequester = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    // Shut on arrival, matching the web capture box's own resting state
    // (`CaptureBox.tsx`'s `detailsOpen`) — a form that opens to seven
    // fields taxes the common case, which is one line and Enter.
    var detailsOpen by rememberSaveable { mutableStateOf(false) }
    // A share that brought a URL in opens the disclosure the Link now lives
    // in, so the reader sees what is about to be saved (#782). An effect,
    // not an initial value: the seed lands after first composition.
    LaunchedEffect(draft.linkOpen) {
        if (draft.linkOpen) detailsOpen = true
    }
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

    // The share's seed (#782), through the core's own mapping and nothing
    // of this file's. `LaunchedEffect(Unit)` re-fires on an Activity
    // recreation, which is why `seedFromShare` is idempotent rather than
    // this effect being keyed — the ViewModel is what survives.
    LaunchedEffect(Unit) {
        if (share != null) {
            viewModel.seedFromShare(parseSharePayload(share.subject, share.text))
        }
    }

    fun submit(destination: CaptureDestination) {
        scope.launch {
            if (viewModel.submit(destination, System.currentTimeMillis())) {
                onFinished()
            }
        }
    }

    Scaffold { padding ->
        // Two children, and the split is the point: the fields scroll, the
        // submit row does not. Before it, the lone button was the last
        // child of the scrolling column, so a raised keyboard could push
        // the only way out of this screen off the bottom of it.
        // `imePadding()` on the outer column is what lifts the row onto the
        // keyboard's top edge — the Activity is already edge-to-edge with
        // `adjustResize` (AndroidManifest.xml), the same conditions
        // `RecallOverlay` relies on. `consumeWindowInsets(padding)` is not
        // decoration either: `padding()` applies the Scaffold's insets
        // without consuming them, so `imePadding()` below it would add the
        // whole IME inset on top of an already-paid navigation bar — the
        // dead band #614 shipped, in the other direction
        // (`MainActivity`'s NavHost carries the same pair for the same
        // reason).
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .consumeWindowInsets(padding)
                .imePadding(),
        ) {
            Column(
                modifier = Modifier
                    .weight(1f)
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
                        // Enter captures to Triage: the funnel's own default
                        // ([CaptureFormState]'s destination was TRIAGE before
                        // #634's round-4 rework moved it onto the gesture), so
                        // the one-line-and-Enter case is unchanged.
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

                // Energy/Size (the frontier's axes) and Context (the open
                // vocabulary) — every word rendered here comes from
                // `viewModel.formMeta`, never a literal in this file.
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
                // Context (operator decision 2026-08-21): a capture that
                // needs a sentence of its own needs it while the words are
                // still in the head, and a field reachable only behind a
                // disclosure is one the hand does not reach for. The rest of
                // the mint set stays behind the chevron.
                OutlinedTextField(
                    value = draft.description,
                    onValueChange = { viewModel.updateDraft(draft.copy(description = it)) },
                    label = { Text("Description") },
                    modifier = Modifier.fillMaxWidth(),
                )
                // The details disclosure rides at this row's right-hand end
                // (operator decision 2026-08-21) rather than centred on a
                // line of its own below — where the web capture box already
                // keeps it (`.hb-capture-details-toggle` in
                // `shell/responsive.css`), and a row carrying nothing but
                // one chevron spent a whole line of height on it.
                // `CenterVertically` levels it with the field's box rather
                // than its floating label.
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
                    // Everything a mint would ask, behind one disclosure — the web
                    // capture box's own "More details" (`CaptureBox.tsx`), drawn
                    // as a chevron since 2026-08-20 (operator decision, taken on
                    // the capture sheet and carried here so the two surfaces do
                    // not disclose the same field set with two different
                    // controls). `ic_chevron_down` rotated a half-turn when open,
                    // `NowScreen`'s `ColumnHeader` idiom; the words survive as the
                    // `contentDescription`.
                    IconButton(onClick = { detailsOpen = !detailsOpen }) {
                        Icon(
                            painterResource(R.drawable.ic_chevron_down),
                            contentDescription = if (detailsOpen) "Fewer details" else "More details",
                            modifier = Modifier.rotate(if (detailsOpen) 180f else 0f),
                        )
                    }
                }

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
                        // One line for the pair, the capture sheet's own
                        // arrangement (operator decision 2026-08-20).
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
                        // The item's one Link (#782), the disclosure's last
                        // row (operator decision 2026-09-04: on its own row
                        // below the disclosure it cost the shut form a line
                        // of height and pushed the two buttons down). A chain
                        // glyph, shut until tapped — or already open and
                        // filled when a share brought a URL in, which also
                        // opens this disclosure (the `LaunchedEffect` above).
                        LinkField(
                            url = draft.linkUrl,
                            label = draft.linkLabel,
                            onUrlChange = { viewModel.updateDraft(draft.copy(linkUrl = it)) },
                            onLabelChange = { viewModel.updateDraft(draft.copy(linkLabel = it)) },
                            initiallyOpen = draft.linkOpen,
                        )
                    }
                }
            }

            // The two destinations the web capture box offers as
            // Triage/Mint (`CaptureBox.tsx`), as two submit buttons rather
            // than a switch above one (operator decision 2026-08-20): the
            // destination is a property of the gesture, so there is no
            // selected-state to read back or get wrong. Both are drawn as
            // solid coloured halves carrying a glyph and no word (operator
            // decision 2026-08-21, the same change as the web's): triage's
            // own blue with the inbox, brand orange with the plus. The
            // colour and the glyph are the label, so each button's
            // `contentDescription` is the only place its gesture is named.
            // `Sky600` is named directly rather than taken from the scheme
            // because `tertiary` is only the light scheme's blue and one
            // fill has to carry white content in both themes. Both are
            // gated on the in-flight flag as well as the draft: two doors
            // to one `captureFn` is two ways to mint the same words twice
            // ([CaptureViewModel.submitting]).
            val canSubmit = viewModel.canSubmitDraft() && !submitting
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 24.dp, vertical = 12.dp),
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
