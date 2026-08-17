package net.twinion.hummingbird

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import kotlin.random.Random
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import net.twinion.hummingbird.core.CoreHolder
import net.twinion.hummingbird.core.TokenStore
import net.twinion.hummingbird.core.TokenValidation
import net.twinion.hummingbird.push.RegistrationWorker
import net.twinion.hummingbird.ui.theme.HummingbirdTheme
import uniffi.hummingbird_ffi_mobile.MobileTaskHost
import uniffi.hummingbird_ffi_mobile.RunOutcome

// `NowScreen` (M1-6/#504) is this activity's content — the frontier,
// decided core-side and rendered verbatim (`NowScreen.kt`'s own doc). M0's
// proof screen (#141: the embedded core's API version, the mirror's
// active-item count, and one live sync against the authority) moves behind
// the "Status" action rather than being deleted — still the cheapest manual
// check that the generated binding and the loaded `.so` agree. No nav
// library in M1 (recorded deferral, M1-6's brief): a plain `showStatus`
// toggle stands in for the `NavHost` a later milestone would add.
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            HummingbirdTheme {
                AppRoot()
            }
        }
    }
}

// The always-composed content root. The #141 sync cadence (one `user` cycle
// on every foreground resume, plus the 60-second `timer` tick while
// resumed) lives here rather than inside `ProofScreen` — it must run
// whichever of Now/Status is on screen, since Now's own mirror is what it
// keeps fresh and an act taken there is what it flushes. `ProofScreen` was
// the cadence's original, and wrong, home: it only composes while the
// "Status" toggle is on, so hoisting the toggle's *content* without hoisting
// the cadence would leave Now unrefreshed and an act unflushed for up to an
// hour, until `SyncWorker`'s background leg. `syncTick` is this root's only
// hand-off to `NowScreen`: it increments once per completed sync cycle so
// Now re-reads `now_queue` after each one, not only on its own resume.
@Composable
private fun AppRoot() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    var core by remember { mutableStateOf<MobileTaskHost?>(null) }
    var facts by remember { mutableStateOf<CoreFacts?>(null) }
    var statusLine by remember { mutableStateOf<String?>(null) }
    var syncing by remember { mutableStateOf(false) }
    var needsToken by remember { mutableStateOf(false) }
    var syncTick by remember { mutableIntStateOf(0) }
    var showStatus by remember { mutableStateOf(false) }

    suspend fun sync(trigger: String) {
        val host = core ?: return
        syncing = true
        val outcome = host.run(
            System.currentTimeMillis(),
            trigger,
            false,
            Random.nextDouble(),
        )
        val credentialEvent =
            host.takeEvents().any { it.kind == "credential_needed" }
        statusLine = describe(outcome)
        needsToken = credentialEvent ||
            outcome.kind == "no_credential" || outcome.kind == "held"
        facts = readFacts(host)
        syncing = false
        syncTick += 1
    }

    LaunchedEffect(Unit) {
        val host = CoreHolder.get(context)
        core = host
        facts = readFacts(host)
        needsToken = TokenStore.load(context) == null
    }

    // Foreground legs of the #141 sync model: one deliberate cycle on
    // every return to the app, plus the 60-second cadence tick while
    // resumed (ADR-0007's foreground timer, exactly the web client's) —
    // hoisted to this root (not `ProofScreen`) so it runs regardless of
    // which screen is showing (#514 review).
    LifecycleResumeEffect(core) {
        val resumed = scope.launch {
            if (core != null) {
                sync("user")
                while (true) {
                    delay(60_000)
                    sync("timer")
                }
            }
        }
        onPauseOrDispose { resumed.cancel() }
    }

    if (showStatus) {
        ProofScreen(
            facts = facts,
            statusLine = statusLine,
            syncing = syncing,
            needsToken = needsToken,
            onBack = { showStatus = false },
            onSync = { scope.launch { sync("user") } },
            onSaveToken = { token ->
                scope.launch {
                    TokenStore.save(context, token)
                    core?.pushApiKey(token)
                    needsToken = false
                    // Registration follows the credential (M2/#141):
                    // `registerPushTarget` is the one authority call that
                    // needs the bearer token in hand rather than riding the
                    // sync queue, so an attempt made before a token existed
                    // returned `Unauthorized` and stopped. This arrival is
                    // the event that makes it worth trying again.
                    RegistrationWorker.enqueue(context)
                    sync("user")
                }
            },
            onForgetToken = {
                scope.launch {
                    TokenStore.clear(context)
                    core?.clearApiKey()
                    needsToken = true
                    statusLine = "No device token — paste one to sync."
                }
            },
        )
    } else {
        NowScreen(onShowStatus = { showStatus = true }, syncTick = syncTick)
    }
}

private data class CoreFacts(
    val apiVersion: UInt,
    val activeItems: UInt,
    val queueDepth: UInt,
)

private suspend fun readFacts(core: MobileTaskHost) = CoreFacts(
    apiVersion = core.apiVersion(),
    activeItems = core.activeItemCount(),
    queueDepth = core.queueDepth(),
)

/** The run-outcome line, in the product's own honest register. */
private fun describe(outcome: RunOutcome): String = when (outcome.kind) {
    "completed" ->
        if (outcome.deadLettered != null && outcome.deadLettered!! > 0u) {
            "Synced — ${outcome.deadLettered} edit(s) didn't apply."
        } else {
            "Synced."
        }
    "skipped" -> "Skipped — backing off after a failure."
    "no_credential" -> "No device token — paste one to sync."
    "held", "credential_needed" -> "Device token rejected — paste a fresh one."
    "blocked" -> "A queued edit is failing; sync stopped early."
    "pull_failed" -> "The authority couldn't be reached."
    "persist_failed" -> "Couldn't persist sync state."
    else -> outcome.kind
}

// The M0 proof screen's display, with no state or cadence of its own — both
// live in `AppRoot` now (#514 review), since the cadence must keep running
// while this screen isn't the one on top.
@Composable
private fun ProofScreen(
    facts: CoreFacts?,
    statusLine: String?,
    syncing: Boolean,
    needsToken: Boolean,
    onBack: () -> Unit,
    onSync: () -> Unit,
    onSaveToken: (String) -> Unit,
    onForgetToken: () -> Unit,
) {
    Scaffold { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            // The product name is lowercase everywhere.
            Text("hummingbird", style = MaterialTheme.typography.headlineLarge)
            TextButton(onClick = onBack) {
                Text("Back to Now")
            }

            if (facts == null) {
                CircularProgressIndicator()
            } else {
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.surface,
                    ),
                ) {
                    Column(
                        modifier = Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        // The mono meta style: data the system computed.
                        Text(
                            "CORE API V${facts.apiVersion} · " +
                                "${facts.activeItems} ACTIVE · " +
                                "${facts.queueDepth} QUEUED",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        statusLine?.let {
                            Text(it, style = MaterialTheme.typography.bodyLarge)
                        }
                    }
                }

                if (needsToken) {
                    TokenEntry(onSave = onSaveToken)
                } else {
                    SyncButton(syncing = syncing, onSync = onSync)
                    TextButton(onClick = onForgetToken) {
                        Text("Forget token")
                    }
                }
            }
        }
    }
}

@Composable
private fun SyncButton(syncing: Boolean, onSync: () -> Unit) {
    Button(onClick = onSync, enabled = !syncing) {
        Text(if (syncing) "Syncing…" else "Sync now")
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
        val normalized = TokenValidation.normalize(raw)
        Button(
            onClick = { normalized?.let(onSave) },
            enabled = normalized != null,
            modifier = Modifier.align(Alignment.End),
        ) {
            Text("Save token")
        }
    }
}
