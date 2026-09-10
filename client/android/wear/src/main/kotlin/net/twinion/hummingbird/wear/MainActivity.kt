package net.twinion.hummingbird.wear

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.wear.compose.material3.AppScaffold
import androidx.wear.compose.navigation.SwipeDismissableNavHost
import androidx.wear.compose.navigation.composable
import androidx.wear.compose.navigation.rememberSwipeDismissableNavController
import kotlin.random.Random
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import net.twinion.hummingbird.core.CoreHolder
import net.twinion.hummingbird.core.SyncHistoryStore
import net.twinion.hummingbird.wear.questions.QuestionsScreen
import net.twinion.hummingbird.wear.token.TokenPresence
import net.twinion.hummingbird.wear.ui.theme.WearTheme
import uniffi.hummingbird_ffi_mobile.MobileTaskHost
import uniffi.hummingbird_ffi_mobile.isInformativeSyncOutcome

// The watch's one activity (ADR-0039). Two routes — the home screen (the
// capture button, the questions button, the one honest line) and the
// standing-questions list — inside Wear's swipe-to-dismiss NavHost, under
// the foreground sync leg this root owns exactly as the phone's `AppRoot`
// does: one deliberate cycle on every resume, then the 60-second cadence
// tick while resumed (ADR-0007's foreground timer). Hoisted here so it runs
// whichever route is showing, and so `syncTick` — this root's only hand-off
// to the screens — bumps once per completed cycle for them to re-read the
// mirror after each one.
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { WearTheme { WearAppRoot() } }
    }
}

@Composable
private fun WearAppRoot() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val navController = rememberSwipeDismissableNavController()

    var core by remember { mutableStateOf<MobileTaskHost?>(null) }
    var syncTick by remember { mutableIntStateOf(0) }
    // The home line's two inputs. `tokenPresent` is the stored fact
    // (`TokenPresence`, refreshed by the listener when the phone sends one);
    // `credentialRefused` is the last cycle's verdict on it. Either missing
    // or refused reads as "send a token from the phone" — the same rule as
    // the phone's `needsToken`, split in two because the watch has a second
    // writer (the listener) the phone does not.
    val tokenPresent by TokenPresence.presentFlow.collectAsStateWithLifecycle()
    var credentialRefused by remember { mutableStateOf(false) }
    var lastInformativeSyncMs by remember { mutableStateOf<Long?>(null) }

    LaunchedEffect(Unit) {
        TokenPresence.refresh(context)
        lastInformativeSyncMs = SyncHistoryStore.load(context).latestInformativeAtMs
        core = CoreHolder.get(context)
    }
    // A token arriving clears the previous refusal: the next cycle judges
    // the new one, and until then the line must not accuse it.
    LaunchedEffect(tokenPresent) {
        if (tokenPresent) credentialRefused = false
    }

    suspend fun sync(trigger: String) {
        val host = core ?: return
        val nowMs = System.currentTimeMillis()
        val outcome = host.run(nowMs, trigger, false, Random.nextDouble())
        val credentialEvent = host.takeEvents().any { it.kind == "credential_needed" }
        credentialRefused = credentialEvent ||
            outcome.kind == "no_credential" || outcome.kind == "held"
        if (isInformativeSyncOutcome(outcome.kind)) {
            lastInformativeSyncMs = nowMs
            // The reachability facts' durable copy, as on the phone (#536).
            SyncHistoryStore.recordInformative(context, outcome.kind, nowMs)
        }
        syncTick += 1
    }

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

    // `syncTick` is what re-ages the line: the clock is read once per
    // completed cycle, never on its own timer.
    val nowMs = remember(syncTick) { System.currentTimeMillis() }

    AppScaffold {
        SwipeDismissableNavHost(navController = navController, startDestination = "home") {
            composable("home") {
                HomeScreen(
                    needsToken = !tokenPresent || credentialRefused,
                    syncAgeLine = syncAgeLine(lastInformativeSyncMs, nowMs),
                    onCapture = { /* the capture flow lands with the next slice */ },
                    onQuestions = { navController.navigate("questions") },
                )
            }
            composable("questions") {
                QuestionsScreen(core = core, syncTick = syncTick)
            }
        }
    }
}
