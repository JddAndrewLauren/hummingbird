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
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.theme.ThemePreference
import uniffi.hummingbird_ffi_mobile.MobileBindingRecord
import uniffi.hummingbird_ffi_mobile.MobileBindingValue
import uniffi.hummingbird_ffi_mobile.MobileDeadLetterReason
import uniffi.hummingbird_ffi_mobile.MobileDeadLetterRecord
import uniffi.hummingbird_ffi_mobile.MobileSyncStatusTone

// The Settings screen (#535/M4): the bindings editor, device-token entry
// and forget (moved off the debug `ProofScreen`), the sync-status card, the
// dead-letter rows, a theme preference, and calendar connect's honest
// "not on this device yet" card — calendar connect itself is out of this
// plan entirely (#527's "Out of scope").
//
// **This file decides nothing about sync status or a binding write's
// outcome.** `syncSummary`/`deadLetterHeadingText` arrive applied from
// `SettingsViewModel`, itself a thin door onto
// `hummingbird_core::decisions::settings` — no Kotlin-side classification
// of what "stale"/"held"/"synced" mean, and no Kotlin re-derivation of a
// binding's known/pending/value states. `SettingsScreenStructuralTest`
// reads this file (and `SettingsViewModel.kt`) to keep it that way.
//
// **The route is registered but not reachable from the bar or the More
// sheet** — `RulesScreen`'s own precedent: that is #541's job, and this
// slice stops at the registration so its evidence is JVM tests rather
// than a hardware pass. `ProofScreen`'s "Manage device token in Settings"
// link is the one way in today.

@Composable
fun SettingsScreen(
    syncTick: Int = 0,
    needsToken: Boolean,
    onSaveToken: (String) -> Unit,
    onForgetToken: () -> Unit,
    themePreference: ThemePreference,
    onThemePreference: (ThemePreference) -> Unit,
    onBack: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val viewModel: SettingsViewModel = viewModel(factory = SettingsViewModel.factory(context))
    val bindings by viewModel.bindings.collectAsState()
    val deadLetters by viewModel.deadLetters.collectAsState()
    val queueDepth by viewModel.queueDepth.collectAsState()
    val bindingError by viewModel.bindingError.collectAsState()

    suspend fun reload() = viewModel.load()

    LaunchedEffect(Unit) { reload() }

    LifecycleResumeEffect(Unit) {
        val resumed = scope.launch { reload() }
        onPauseOrDispose { resumed.cancel() }
    }

    LaunchedEffect(syncTick) {
        if (syncTick > 0) reload()
    }

    val nowMs = System.currentTimeMillis()
    val summary = viewModel.syncSummary(nowMs)

    Scaffold { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp)
                .verticalScroll(rememberScrollState()),
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

            SectionTitle("Calendar")
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            ) {
                Text(
                    "Calendar context isn't available on this device yet.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(16.dp),
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
                    Text(
                        summary.label,
                        style = MaterialTheme.typography.bodyLarge,
                        color = syncStatusToneColor(summary.tone),
                    )
                    Text(
                        "$queueDepth queued",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Button(onClick = { scope.launch { viewModel.sync(System.currentTimeMillis()) } }) {
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

@Composable
private fun syncStatusToneColor(tone: MobileSyncStatusTone) = when (tone) {
    MobileSyncStatusTone.NEUTRAL -> MaterialTheme.colorScheme.onSurfaceVariant
    MobileSyncStatusTone.WARN -> MaterialTheme.colorScheme.tertiary
    MobileSyncStatusTone.DANGER -> MaterialTheme.colorScheme.error
    MobileSyncStatusTone.SUCCESS -> MaterialTheme.colorScheme.onSurface
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
