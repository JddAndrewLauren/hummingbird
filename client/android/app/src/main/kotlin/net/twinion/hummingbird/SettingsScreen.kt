package net.twinion.hummingbird

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.core.NetworkStatus
import net.twinion.hummingbird.skills.BackendPreference
import net.twinion.hummingbird.theme.ThemePreference
import net.twinion.hummingbird.ui.theme.Amber600
import net.twinion.hummingbird.ui.theme.LocalHbDark
import net.twinion.hummingbird.ui.theme.Moss600
import net.twinion.hummingbird.ui.theme.StatusDoneFgDark
import net.twinion.hummingbird.ui.theme.StatusWarnFgDark
import uniffi.hummingbird_ffi_mobile.MobileBindingRecord
import uniffi.hummingbird_ffi_mobile.MobileBindingValue
import uniffi.hummingbird_ffi_mobile.MobileCalendarConnection
import uniffi.hummingbird_ffi_mobile.MobileCalendarList
import uniffi.hummingbird_ffi_mobile.MobileCalendarSelection
import uniffi.hummingbird_ffi_mobile.MobileCalendarState
import uniffi.hummingbird_ffi_mobile.MobileDeadLetterReason
import uniffi.hummingbird_ffi_mobile.MobileDeadLetterRecord
import uniffi.hummingbird_ffi_mobile.MobileSyncStatusInput
import uniffi.hummingbird_ffi_mobile.MobileSyncStatusSummary
import uniffi.hummingbird_ffi_mobile.MobileSyncStatusTone
import uniffi.hummingbird_ffi_mobile.syncStatusSummary

// The Settings screen (#535/M4): the bindings editor, device-token entry
// and forget (moved off the debug `ProofScreen`), the sync-status card, the
// dead-letter rows, a theme preference, and calendar connect's honest
// "not on this device yet" card — calendar connect itself is out of this
// plan entirely (#527's "Out of scope").
//
// **This file decides nothing about sync status or a binding write's
// outcome.** `syncSummary`/`SettingsViewModel.deadLetterHeadingText` arrive
// applied from `hummingbird_core::decisions::settings` — no Kotlin-side
// classification of what "stale"/"held"/"synced" mean, and no Kotlin
// re-derivation of a binding's known/pending/value states.
// `SettingsScreenStructuralTest` reads this file (and
// `SettingsViewModel.kt`) to keep it that way.
//
// **`lastSyncOutcomeKind`/`lastSyncAtMs` arrive from `AppRoot`, not from
// this screen's own `SettingsViewModel`** (round-1 review, #535): the real
// sync cadence — one `user` cycle per resume plus the 60-second `timer`
// loop — runs above the `NavHost`, and a `viewModel()` here is rebuilt
// every time its `NavBackStackEntry` is left and re-entered. Reading only
// this screen's own state would read "Not yet synced" on almost every real
// visit, however long the app had actually been syncing. `onSync` is
// `AppRoot`'s own `sync("user")`, for the same reason: one cadence, one
// writer.
//
// **The route has a permanent More-sheet entry since #541**, alongside its
// pre-existing incidental door — `StatusScreen`'s own "Manage device token
// in Settings" link (#536 review), which stays.

@Composable
fun SettingsScreen(
    syncTick: Int = 0,
    needsToken: Boolean,
    onSaveToken: (String) -> Unit,
    onForgetToken: () -> Unit,
    themePreference: ThemePreference,
    onThemePreference: (ThemePreference) -> Unit,
    /** The real cadence's last completed, informative cycle — `AppRoot`'s
     * own state, threaded down exactly as `syncTick` already is. */
    lastSyncOutcomeKind: String?,
    lastSyncAtMs: Long?,
    onSync: () -> Unit,
    /** #564's calendar lane. The connection is `AppRoot`'s state for the
     * same reason the sync card's is (this file's own doc above): the
     * cadence that maintains it runs above the `NavHost`, and a
     * screen-scoped copy would read "never connected" on every visit. The
     * picker underneath it is this screen's own — it is a preference
     * editor, not a cadence. */
    calendarConnection: MobileCalendarConnection,
    onConnectCalendar: () -> Unit,
    onDisconnectCalendar: () -> Unit,
    onBack: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val viewModel: SettingsViewModel = viewModel(factory = SettingsViewModel.factory(context))
    val bindings by viewModel.bindings.collectAsState()
    val deadLetters by viewModel.deadLetters.collectAsState()
    val queueDepth by viewModel.queueDepth.collectAsState()
    val bindingError by viewModel.bindingError.collectAsState()
    val calendars by viewModel.calendars.collectAsState()
    val calendarSelections by viewModel.calendarSelections.collectAsState()

    suspend fun reload() = viewModel.load()

    // Re-listed whenever the connection state moves: a device that has just
    // connected has a credential the previous list attempt did not, and one
    // that has just been refused should stop showing options it can no
    // longer read back.
    LaunchedEffect(calendarConnection.state) { viewModel.loadCalendars() }

    LaunchedEffect(Unit) { reload() }

    LifecycleResumeEffect(Unit) {
        val resumed = scope.launch { reload() }
        onPauseOrDispose { resumed.cancel() }
    }

    LaunchedEffect(syncTick) {
        if (syncTick > 0) reload()
    }

    // `hummingbird_core::decisions::settings::sync_status_summary`, called
    // straight from render — the same shape `notificationTapTarget`'s call
    // in `AppRoot` already takes for a synchronous, clock-free decision
    // with no durable state of its own to hold.
    val summary: MobileSyncStatusSummary = syncStatusSummary(
        MobileSyncStatusInput(
            online = NetworkStatus.isOnline(context),
            lastSyncOutcomeKind = lastSyncOutcomeKind,
            lastSyncAtMs = lastSyncAtMs,
            queueDepth = queueDepth,
            nowMs = System.currentTimeMillis(),
        ),
    )

    Scaffold { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp)
                .verticalScroll(rememberScrollState())
                // Scrolled, not a fixed inset: the last control clears the
                // Capture FAB (24dp outer + this).
                .padding(bottom = 64.dp),
            verticalArrangement = Arrangement.spacedBy(24.dp),
        ) {
            TextButton(onClick = onBack) { Text("Back") }
            Text("Settings", style = MaterialTheme.typography.headlineLarge)

            SectionTitle("Standing questions")
            val currentBindings = bindings
            if (currentBindings == null) {
                CircularProgressIndicator()
            } else {
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
                ) {
                    Column(
                        modifier = Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(20.dp),
                    ) {
                        for (binding in currentBindings) {
                            BindingRow(
                                binding = binding,
                                writeError = bindingError?.takeIf { it.first == binding.key }?.second,
                                onSave = { value ->
                                    scope.launch { viewModel.setBinding(binding.key, value, System.currentTimeMillis()) }
                                },
                            )
                        }
                    }
                }
            }

            SectionTitle("This device")
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            ) {
                Column(
                    modifier = Modifier.padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Text(
                        "THEME",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        for (preference in ThemePreference.entries) {
                            if (preference == themePreference) {
                                Button(onClick = { onThemePreference(preference) }) {
                                    Text(themePreferenceLabel(preference))
                                }
                            } else {
                                OutlinedButton(onClick = { onThemePreference(preference) }) {
                                    Text(themePreferenceLabel(preference))
                                }
                            }
                        }
                    }
                }
            }

            SectionTitle("Skills backend")
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            ) {
                Column(
                    modifier = Modifier.padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Text(
                        "Which runner a Grill turn or a microtask run is attempted against.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    BackendPicker(context = context)
                }
            }

            SectionTitle("Calendar")
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            ) {
                CalendarSection(
                    connection = calendarConnection,
                    calendars = calendars,
                    selections = calendarSelections,
                    onConnect = onConnectCalendar,
                    onDisconnect = onDisconnectCalendar,
                    onToggleCalendar = { id -> scope.launch { viewModel.toggleCalendar(id) } },
                )
            }

            SectionTitle("Device token")
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            ) {
                Column(
                    modifier = Modifier.padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    if (needsToken) {
                        TokenEntry(onSave = onSaveToken)
                    } else {
                        Text(
                            "This device has a token.",
                            style = MaterialTheme.typography.bodyMedium,
                        )
                        OutlinedButton(onClick = onForgetToken) { Text("Forget token") }
                    }
                }
            }

            SectionTitle("Sync")
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            ) {
                Column(
                    modifier = Modifier.padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    // `summary.label` already carries the queued count —
                    // `decisions::settings::queued_suffix`'s own rule is
                    // that a "0 queued" pill is decoration, so a second,
                    // Kotlin-side "$queueDepth queued" line would both
                    // double the count when something is queued and print
                    // exactly the "0 queued" noise the core suppresses.
                    Text(
                        summary.label,
                        style = MaterialTheme.typography.bodyLarge,
                        color = syncStatusToneColor(summary.tone),
                        // `toneWord` is the same fact as `label`/`tone`,
                        // worded for a screen reader rather than a sighted
                        // render — the one Kotlin caller of
                        // `MobileSyncStatusSummary.toneWord` (#535 review).
                        modifier = Modifier.semantics {
                            contentDescription = "Sync status: ${summary.toneWord}. ${summary.label}"
                        },
                    )
                    Button(onClick = onSync) {
                        Text("Sync now")
                    }
                }
            }

            if (deadLetters.isNotEmpty()) {
                SectionTitle(viewModel.deadLetterHeadingText())
                Card(modifier = Modifier.fillMaxWidth()) {
                    Column(
                        modifier = Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(16.dp),
                    ) {
                        for (entry in deadLetters) {
                            DeadLetterRow(entry)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun SectionTitle(text: String) {
    Text(
        text.uppercase(),
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** One binding row: what it's for, what it holds, and — for a key this
 * build can write — a field and a Save button. A row for an unknown key
 * renders the value and stops, the same "no field, no button" rule
 * `bindings.ts`'s `BindingRow` follows: `settings` has no DELETE, so a key
 * this build cannot name is one it must not overwrite either. */
@Composable
private fun BindingRow(
    binding: MobileBindingRecord,
    writeError: String?,
    onSave: (String) -> Unit,
) {
    var draft by remember(binding.key) { mutableStateOf(bindingDraftSeed(binding.value)) }
    // Reseed the field whenever the value underneath it moves — a sync
    // carrying another device's edit, or this device's own write
    // confirming. The row is remembered by binding key and so survives
    // every re-read, which means the seed alone would leave a stale draft
    // sitting over a value it never showed, with Save enabled to push it
    // back. `SettingsScreen.tsx`'s `BindingRow` does exactly this (#118
    // review finding); #565's review found the phone had inherited the
    // seed without the reseed.
    var seenValue by remember(binding.key) { mutableStateOf(binding.value) }
    if (seenValue != binding.value) {
        seenValue = binding.value
        draft = bindingDraftSeed(binding.value)
    }

    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(binding.key, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (binding.pending) {
                BadgedBox(badge = { Badge { Text("queued") } }) {}
            }
        }
        Text(bindingValueLabel(binding.value), style = MaterialTheme.typography.bodyMedium)
        if (binding.known) {
            OutlinedTextField(
                value = draft,
                onValueChange = { draft = it },
                modifier = Modifier.fillMaxWidth(),
            )
            if (writeError != null) {
                Text(writeError, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
            }
            Button(
                onClick = { onSave(draft.trim()) },
                enabled = draft.isNotBlank() && !sameBindingText(binding.value, draft.trim()),
            ) {
                Text("Save")
            }
        }
    }
}

/** One dead-lettered entry's field-level detail — the conflict arm the
 * brief asks for, rendered rather than silently retried: a
 * [MobileDeadLetterReason.Conflict] shows the local and server value for
 * every field that disagreed. */
@Composable
private fun DeadLetterRow(entry: MobileDeadLetterRecord) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(
            "${entry.entity}${entry.entityId?.let { ":$it" } ?: ""}",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        when (val reason = entry.reason) {
            is MobileDeadLetterReason.Permanent -> Text(
                reason.detail,
                style = MaterialTheme.typography.bodyMedium,
            )
            MobileDeadLetterReason.Conflict -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(
                    "This device's change conflicted with one already saved:",
                    style = MaterialTheme.typography.bodyMedium,
                )
                for (field in entry.fields) {
                    Text(
                        "${field.field} — local: ${field.localJson}, server: ${field.serverJson}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            MobileDeadLetterReason.Contention -> Text(
                "This change kept colliding with others and was given up on.",
                style = MaterialTheme.typography.bodyMedium,
            )
        }
    }
}

/** #274's picker, landed here at #539 — Auto plus every entry
 * [BackendPreference.REGISTRY] carries (this slice's one, the cloud
 * runner). The stored selection is [BackendPreference]'s own
 * degrade-to-Auto rule ([uniffi.hummingbird_ffi_mobile.resolveBackendSelection],
 * sunk to the core so a stale pin naming a retired tier never renders a
 * selection this picker cannot label. */
@Composable
private fun BackendPicker(context: android.content.Context) {
    var selection by remember { mutableStateOf(BackendPreference.read(context)) }

    fun select(next: String) {
        BackendPreference.write(context, next)
        selection = next
    }

    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        BackendOption(
            label = "Auto",
            selected = selection == BackendPreference.AUTO,
            onClick = { select(BackendPreference.AUTO) },
        )
        for (entry in BackendPreference.ENTRIES) {
            BackendOption(
                label = entry.label,
                selected = selection == entry.id,
                onClick = { select(entry.id) },
            )
        }
    }
}

@Composable
private fun BackendOption(label: String, selected: Boolean, onClick: () -> Unit) {
    if (selected) {
        Button(onClick = onClick) { Text(label) }
    } else {
        OutlinedButton(onClick = onClick) { Text(label) }
    }
}

@Composable
private fun TokenEntry(onSave: (String) -> Unit) {
    var raw by remember { mutableStateOf("") }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            "Paste this device's token. It is stored in the Android " +
                "Keystore and sent only to the authority.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        OutlinedTextField(
            value = raw,
            onValueChange = { raw = it },
            modifier = Modifier.fillMaxWidth(),
            label = { Text("Device token") },
            singleLine = true,
        )
        val normalized = net.twinion.hummingbird.core.TokenValidation.normalize(raw)
        Button(onClick = { normalized?.let(onSave) }, enabled = normalized != null) {
            Text("Save token")
        }
    }
}

// -- rendering, and only rendering -------------------------------------
//
// Two clients wording a theme choice or a sync tone differently is a
// difference, not a bug — the same line ADR-0025's verdict table draws for
// `rules/operators.ts`'s `OPERATOR_LABELS`.

private fun themePreferenceLabel(preference: ThemePreference): String = when (preference) {
    ThemePreference.SYSTEM -> "Follow system"
    ThemePreference.LIGHT -> "Light"
    ThemePreference.DARK -> "Dark"
}

// `tertiary` is the design system's *info* blue (`--status-info-fg`,
// `Theme.kt`'s own mapping note) — wrong for WARN, which reads "Held —
// device token needed" and needs `--status-warn-fg`. `onSurface` is plain
// body text and indistinguishable from every other line on the card —
// wrong for SUCCESS, which needs `--status-done-fg` (#535 review).
@Composable
private fun syncStatusToneColor(tone: MobileSyncStatusTone): Color {
    val dark = LocalHbDark.current
    return when (tone) {
        MobileSyncStatusTone.NEUTRAL -> MaterialTheme.colorScheme.onSurfaceVariant
        MobileSyncStatusTone.WARN -> if (dark) StatusWarnFgDark else Amber600
        MobileSyncStatusTone.DANGER -> MaterialTheme.colorScheme.error
        MobileSyncStatusTone.SUCCESS -> if (dark) StatusDoneFgDark else Moss600
    }
}

/** The text a row's input starts at — the current value when it is text,
 * empty otherwise. Never pre-loaded for `Other`: a value this editor
 * cannot express must not seed a field whose Save would overwrite it with
 * a mangled string. `bindings.ts`'s `bindingDraftSeed`, read the same way
 * here — a rendering helper, not a decision, since it never asks whether
 * the value is legal, only what a text field starts at. */
private fun bindingDraftSeed(value: MobileBindingValue): String = when (value) {
    is MobileBindingValue.Text -> value.text
    is MobileBindingValue.Unset, is MobileBindingValue.Other -> ""
}

private fun bindingValueLabel(value: MobileBindingValue): String = when (value) {
    MobileBindingValue.Unset -> "Not set"
    is MobileBindingValue.Text -> value.text
    is MobileBindingValue.Other -> "Not a text value: ${value.raw}"
}

private fun sameBindingText(value: MobileBindingValue, draft: String): Boolean =
    value is MobileBindingValue.Text && value.text == draft

// -------------------------------------------------------- calendar (#564)
// The Calendar section: one Connect/Disconnect control, a picker over the
// calendars this device's credential can read, and one sentence of state.
//
// **This section decides nothing.** Which of the four Source-connection
// states the device is in arrives applied as a [MobileCalendarState] from
// `ffi-mobile`'s `calendar_token::connection_state`; the `when` below only
// picks words for it. There is no Kotlin test of an error code anywhere in
// this file — the codes never reach it, only the state does.
//
// **No token is rendered, held or logged here**, and none can be: the
// Google access token never crosses the seam at all (`ffi-mobile`'s
// calendar section header), and the device token is `TokenStore`'s.

/** Whether Connect is on offer. **Only `NEVER_CONNECTED`** — the whole
 * point of the *cannot confirm* state is that an offline or
 * authority-down device reads as connected and keeps showing its mirror,
 * so re-offering Connect there would invite the operator to "fix"
 * something no tap can fix. A refusal offers its own remedy instead
 * (below), which is the token control or nothing. */
private fun offersConnect(state: MobileCalendarState): Boolean =
    state == MobileCalendarState.NEVER_CONNECTED

/** One sentence per state — never shared between two of them, which is
 * #564's own acceptance criterion: *never connected* and *refused* reading
 * the same way is exactly how a broken lane hides as an un-set-up one. */
private fun calendarStateSentence(state: MobileCalendarState): String = when (state) {
    MobileCalendarState.NEVER_CONNECTED ->
        "Not connected. Connecting lets the weekend and vacation questions read your calendar."
    MobileCalendarState.CONNECTED ->
        "Connected. Your calendar is read in the background every so often."
    MobileCalendarState.CANNOT_CONFIRM ->
        "Still connected, but this device can't reach the server right now — " +
            "what's shown may be out of date."
    MobileCalendarState.REFUSED_DEVICE_TOKEN ->
        "The server wouldn't accept this device's token. Set a fresh one under Device token below."
    MobileCalendarState.REFUSED_SERVER_LANE ->
        "The server can't hand out calendar access at the moment. Nothing to fix on this device."
}

@Composable
private fun CalendarSection(
    connection: MobileCalendarConnection,
    calendars: MobileCalendarList?,
    selections: List<MobileCalendarSelection>,
    onConnect: () -> Unit,
    onDisconnect: () -> Unit,
    onToggleCalendar: (String) -> Unit,
) {
    Column(
        modifier = Modifier.padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(
            calendarStateSentence(connection.state),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )

        if (offersConnect(connection.state)) {
            Button(onClick = onConnect) { Text("Connect calendar") }
        } else {
            OutlinedButton(onClick = onDisconnect) { Text("Disconnect calendar") }
        }

        // The picker. A list that could not be read leaves whatever is
        // already selected alone rather than clearing it — the same rule
        // `CalendarHostCore::list_calendars` states for its own answer.
        val options = calendars?.takeIf { it.kind == "ok" }?.calendars.orEmpty()
        if (options.isNotEmpty()) {
            Text("Calendars to read", style = MaterialTheme.typography.titleSmall)
            for (entry in options) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .semantics { contentDescription = "calendar ${entry.summary}" },
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Checkbox(
                        checked = selections.any { it.id == entry.id },
                        onCheckedChange = { onToggleCalendar(entry.id) },
                    )
                    Text(entry.summary, style = MaterialTheme.typography.bodyMedium)
                }
            }
        } else if (connection.state == MobileCalendarState.CONNECTED) {
            Text(
                "No calendars came back to choose from.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
