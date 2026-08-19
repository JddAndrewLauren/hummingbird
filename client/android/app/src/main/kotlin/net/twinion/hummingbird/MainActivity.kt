package net.twinion.hummingbird

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.isSystemInDarkTheme
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
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import kotlin.random.Random
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import net.twinion.hummingbird.core.CoreHolder
import net.twinion.hummingbird.core.TokenStore
import net.twinion.hummingbird.notify.AlertNotifier
import net.twinion.hummingbird.notify.NotificationChannels
import net.twinion.hummingbird.push.RegistrationWorker
import net.twinion.hummingbird.theme.ThemePreference
import net.twinion.hummingbird.theme.ThemeStore
import net.twinion.hummingbird.theme.resolveDarkTheme
import net.twinion.hummingbird.ui.theme.HummingbirdTheme
import uniffi.hummingbird_ffi_mobile.MobileTapTarget
import uniffi.hummingbird_ffi_mobile.MobileTaskHost
import uniffi.hummingbird_ffi_mobile.isInformativeSyncOutcome
import uniffi.hummingbird_ffi_mobile.notificationTapTarget
import uniffi.hummingbird_ffi_mobile.RunOutcome

// `NowScreen` (M1-6/#504) is this activity's start destination — the
// frontier, decided core-side and rendered verbatim (`NowScreen.kt`'s own
// doc). M0's proof screen (#141: the embedded core's API version, the
// mirror's active-item count, and one live sync against the authority)
// lives behind the "Status" action rather than being deleted — still the
// cheapest manual check that the generated binding and the loaded `.so`
// agree.
//
// **M1 deferred a nav library; M2 adopts one.** The recorded deferral was
// that a `showStatus` boolean stood in for the `NavHost` a later milestone
// would add. That milestone is this one, and the forcing function is the
// notification deep link: a tapped alert has to land on *that alert's*
// detail screen with a back stack that returns somewhere sensible, and
// `alert/{alertId}` is an argument a boolean cannot carry. Five routes
// since ADR-0027 added `item/{itemId}`, the second notification
// destination; no nested graphs.
//
// The intent extras, not `navDeepLink`, carry the tap. Android 12+ bans
// notification trampolines, so it already arrives as an Activity intent
// (`AlertNotifier`) — reading its extras is the direct expression of what
// actually happens, where a URI deep link would be a second encoding of the
// same fact. Which destination those extras lead to is the core's answer
// (ADR-0027), never parsed here.
class MainActivity : ComponentActivity() {

    /** The launching (or newly delivered) notification intent, as the
     * three strings the tap decision needs. A flow rather than a Compose
     * state because `onNewIntent` fires outside composition — the Activity
     * is already running when a second notification is tapped. */
    private val deepLinkedAlertId = MutableStateFlow<NotificationTap?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        deepLinkedAlertId.value = NotificationTap.from(intent)
        setContent {
            // The theme preference (#535) is read here, above
            // `HummingbirdTheme`, rather than inside `AppRoot` — the
            // theme call needs the resolved `darkTheme` boolean before it
            // composes anything beneath it, and `AppRoot` is that
            // "beneath". `ThemeStore` is plain `SharedPreferences`
            // (device-local, no secret), so the initial read happens
            // straight in composition rather than behind a
            // `LaunchedEffect` — the same reasoning `HummingbirdTheme`'s
            // own `isSystemInDarkTheme()` default already relies on.
            val context = LocalContext.current
            var themePreference by remember { mutableStateOf(ThemeStore.load(context)) }
            HummingbirdTheme(darkTheme = resolveDarkTheme(themePreference, isSystemInDarkTheme())) {
                AppRoot(
                    deepLinkedAlertId = deepLinkedAlertId,
                    themePreference = themePreference,
                    onThemePreference = { preference ->
                        ThemeStore.save(context, preference)
                        themePreference = preference
                    },
                )
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        NotificationTap.from(intent)?.let {
            deepLinkedAlertId.value = it
        }
    }
}

/** One tapped notification, as the extras `AlertNotifier` put on the
 * intent. Nothing here interprets `source`/`sourceKey`: they are handed
 * to the core's `notificationTapTarget`, which owns the answer (ADR-0027).
 * Absent extras cross as empty strings, which that function answers with
 * alert detail — the permanent contract for an alert naming no item. */
private data class NotificationTap(
    val alertId: String,
    val source: String,
    val sourceKey: String,
) {
    companion object {
        fun from(intent: Intent?): NotificationTap? {
            val alertId = intent?.getStringExtra(AlertNotifier.EXTRA_ALERT_ID) ?: return null
            return NotificationTap(
                alertId = alertId,
                source = intent.getStringExtra(AlertNotifier.EXTRA_SOURCE).orEmpty(),
                sourceKey = intent.getStringExtra(AlertNotifier.EXTRA_SOURCE_KEY).orEmpty(),
            )
        }
    }
}

/** The seven routes. Strings, because that is what `NavHost` takes; kept in
 * one place so a typo is a compile error at the use site rather than a
 * silently unreachable screen.
 *
 * [RULES], [TRIAGE] and [SETTINGS] are **registered and not yet
 * reachable** — no bar entry, no More sheet, no other screen navigates to
 * any of them (#540/#531/#535, M3/M4), except [SETTINGS]'s one incidental
 * door via `ProofScreen`'s "Manage device token in Settings" link, since
 * token entry/forget moved off that debug surface in this slice. None has
 * a permanent nav entry yet. Reachability is #532's job for Triage and
 * #541's for Rules and Settings, along with the nav form that will carry
 * them; registering the route here first is what lets each screen exist,
 * compile and be gated without inventing a navigation shape this slice has
 * not decided. */
private object Routes {
    const val NOW = "now"
    const val STATUS = "status"
    const val ALERTS = "alerts"
    const val RULES = "rules"
    const val TRIAGE = "triage"
    const val SETTINGS = "settings"
    const val ALERT_DETAIL = "alert/{alertId}"
    const val ITEM_DETAIL = "item/{itemId}"
    const val GRILL = "grill/{itemId}/{from}"

    fun alertDetail(alertId: String) = "alert/$alertId"

    fun itemDetail(itemId: String) = "item/$itemId"

    /** [from] is `"triage"` or `"detail"` — the takeover's one nav arg, and
     * the whole of how [GrillTakeoverScreen] knows which surface's own
     * words its Back control names (#355 review round 1's own rule, ported
     * from the web's two `backLabel` call sites). */
    fun grill(itemId: String, from: String) = "grill/$itemId/$from"
}

// The always-composed content root. The #141 sync cadence (one `user` cycle
// on every foreground resume, plus the 60-second `timer` tick while
// resumed) lives here rather than inside any one screen — it must run
// whichever route is on screen, since Now's own mirror is what it keeps
// fresh and an act taken there is what it flushes. `ProofScreen` was the
// cadence's original, and wrong, home: it only composed while the "Status"
// toggle was on, so hoisting the toggle's *content* without hoisting the
// cadence would leave Now unrefreshed and an act unflushed for up to an
// hour, until `SyncWorker`'s background leg. That reasoning (#514) is
// route-independent, which is why adopting a `NavHost` moves nothing here:
// the cadence stays above it, not inside a destination.
//
// `syncTick` is this root's only hand-off to the screens: it increments
// once per completed sync cycle so they re-read the mirror after each one,
// not only on their own resume.
@Composable
private fun AppRoot(
    deepLinkedAlertId: MutableStateFlow<NotificationTap?>,
    themePreference: ThemePreference,
    onThemePreference: (ThemePreference) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val navController = rememberNavController()

    var core by remember { mutableStateOf<MobileTaskHost?>(null) }
    var facts by remember { mutableStateOf<CoreFacts?>(null) }
    var statusLine by remember { mutableStateOf<String?>(null) }
    var syncing by remember { mutableStateOf(false) }
    var needsToken by remember { mutableStateOf(false) }
    var syncTick by remember { mutableIntStateOf(0) }
    // #535 review: the sync card's real input. Held here, above the
    // `NavHost`, for the same reason the cadence itself is (`AppRoot`'s
    // own doc below) — a screen-scoped `ViewModel` is rebuilt every time
    // its `NavBackStackEntry` is left and re-entered, so state that lived
    // only in `SettingsViewModel` reset to "Not yet synced" on every
    // return to Settings even after ten minutes of the app's own resume/
    // 60-second cadence syncing happily. `AppRoot` is the one place that
    // sees every cycle, whichever route is on screen. Only an
    // *informative* outcome overwrites either — a backed-off
    // `"skipped"`/`"busy"` tick must never read as a fresh "Synced".
    var lastSyncOutcomeKind by remember { mutableStateOf<String?>(null) }
    var lastSyncAtMs by remember { mutableStateOf<Long?>(null) }

    suspend fun sync(trigger: String) {
        val host = core ?: return
        syncing = true
        val nowMs = System.currentTimeMillis()
        val outcome = host.run(
            nowMs,
            trigger,
            false,
            Random.nextDouble(),
        )
        val credentialEvent =
            host.takeEvents().any { it.kind == "credential_needed" }
        statusLine = describe(outcome)
        needsToken = credentialEvent ||
            outcome.kind == "no_credential" || outcome.kind == "held"
        if (isInformativeSyncOutcome(outcome.kind)) {
            lastSyncOutcomeKind = outcome.kind
            lastSyncAtMs = nowMs
        }
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

    NotificationPermissionRequest()

    // The notification deep link. Collected here rather than in a
    // destination because the target destination may not exist yet — the
    // tap is what creates it — and because a second tap while the app is
    // already open arrives through `onNewIntent`, outside any composition.
    LaunchedEffect(navController) {
        deepLinkedAlertId.collect { tap ->
            if (tap != null) {
                // The destination is the core's answer, not this file's:
                // a Kotlin `removePrefix("item:")` would hand-copy a key
                // convention that has one owner (ADR-0027). Synchronous
                // and clock-free, so it runs before navigating.
                when (val target = notificationTapTarget(tap.source, tap.sourceKey)) {
                    is MobileTapTarget.Item ->
                        navController.openItemFromNotification(target.itemId)
                    MobileTapTarget.Alert ->
                        navController.openAlertFromNotification(tap.alertId)
                }
                // Consumed: a configuration change must not re-navigate.
                deepLinkedAlertId.value = null
            }
        }
    }

    // Foreground legs of the #141 sync model: one deliberate cycle on
    // every return to the app, plus the 60-second cadence tick while
    // resumed (ADR-0007's foreground timer, exactly the web client's) —
    // hoisted to this root so it runs regardless of which route is
    // showing (#514 review).
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

    // The channels are asserted at app start too (`HummingbirdApp`), but
    // the DND-access grant is made *outside* the app, in Settings, and a
    // bypassing channel can only be created while the grant is held
    // (NotificationChannels' third note). Coming back is the only signal
    // this process gets that the answer may have changed, so re-assert
    // here. Cheap and idempotent; it is not a second clock, it is a
    // response to an event.
    LifecycleResumeEffect(Unit) {
        NotificationChannels.ensure(context)
        onPauseOrDispose { }
    }

    // The token entry/forget gestures — shared between `ProofScreen`
    // (which no longer carries the widgets themselves, #535) and
    // `SettingsScreen` (their one home now). Hoisted here rather than
    // inlined in either `composable {}` body, the same reason `sync`
    // above is: both destinations reach the same `core`/`needsToken`
    // state, and duplicating the two lambdas per screen would be the
    // Kotlin copy this slice moved away from.
    suspend fun saveToken(token: String) {
        TokenStore.save(context, token)
        core?.pushApiKey(token)
        needsToken = false
        // Registration follows the credential (M2/#141): `registerPushTarget`
        // is the one authority call that needs the bearer token in hand
        // rather than riding the sync queue, so an attempt made before a
        // token existed returned `Unauthorized` and stopped. This arrival
        // is the event that makes it worth trying again.
        RegistrationWorker.enqueue(context)
        sync("user")
    }

    suspend fun forgetToken() {
        TokenStore.clear(context)
        core?.clearApiKey()
        needsToken = true
        statusLine = "No device token — paste one to sync."
    }

    NavHost(navController = navController, startDestination = Routes.NOW) {
        composable(Routes.NOW) {
            NowScreen(
                onShowStatus = { navController.navigate(Routes.STATUS) },
                onShowAlerts = { navController.navigate(Routes.ALERTS) },
                // A plain navigate, deliberately not
                // `openItemFromNotification`: that helper's `popUpTo`
                // exists because a *restored* back stack may already hold
                // debris beneath a cold tap (#518). Here Now is the live
                // destination being navigated from, so it is already
                // directly beneath and popping to it would be a no-op
                // dressed as policy.
                onOpenItem = { itemId -> navController.navigate(Routes.itemDetail(itemId)) },
                syncTick = syncTick,
            )
        }
        composable(Routes.STATUS) {
            ProofScreen(
                facts = facts,
                statusLine = statusLine,
                syncing = syncing,
                needsToken = needsToken,
                onBack = { navController.popBackStack() },
                onSync = { scope.launch { sync("user") } },
                onGoToSettings = { navController.navigate(Routes.SETTINGS) },
            )
        }
        composable(Routes.SETTINGS) {
            SettingsScreen(
                syncTick = syncTick,
                needsToken = needsToken,
                onSaveToken = { token -> scope.launch { saveToken(token) } },
                onForgetToken = { scope.launch { forgetToken() } },
                themePreference = themePreference,
                onThemePreference = onThemePreference,
                // #535 review: the real cadence's own state, not a
                // screen-local copy — see the `lastSyncOutcomeKind`/
                // `lastSyncAtMs` note above `sync()`.
                lastSyncOutcomeKind = lastSyncOutcomeKind,
                lastSyncAtMs = lastSyncAtMs,
                onSync = { scope.launch { sync("user") } },
                onBack = { navController.popBackStack() },
            )
        }
        composable(Routes.ALERTS) {
            AlertsScreen(
                syncTick = syncTick,
                onBack = { navController.popBackStack() },
                onOpenAlert = { alertId ->
                    navController.navigate(Routes.alertDetail(alertId))
                },
            )
        }
        composable(Routes.RULES) {
            RulesScreen(
                syncTick = syncTick,
                onBack = { navController.popBackStack() },
            )
        }
        composable(Routes.TRIAGE) {
            TriageScreen(
                syncTick = syncTick,
                onBack = { navController.popBackStack() },
                onGrill = { itemId -> navController.navigate(Routes.grill(itemId, "triage")) },
            )
        }
        composable(Routes.ITEM_DETAIL) { entry ->
            ItemDetailScreen(
                itemId = entry.arguments?.getString("itemId").orEmpty(),
                syncTick = syncTick,
                onBack = { navController.popBackStackOrHome(Routes.NOW) },
                onGrill = { itemId -> navController.navigate(Routes.grill(itemId, "detail")) },
            )
        }
        composable(Routes.GRILL) { entry ->
            GrillTakeoverScreen(
                itemId = entry.arguments?.getString("itemId").orEmpty(),
                backLabel = if (entry.arguments?.getString("from") == "triage") "Back to Triage" else "Back to item",
                onBack = { navController.popBackStack() },
            )
        }
        composable(Routes.ALERT_DETAIL) { entry ->
            AlertDetailScreen(
                alertId = entry.arguments?.getString("alertId").orEmpty(),
                syncTick = syncTick,
                onBack = { navController.popBackStackOrHome(Routes.NOW) },
            )
        }
    }
}

/** A tapped notification lands its alert directly on top of Now — the back
 * stack is exactly `now -> alert/{id}`, cold or warm.
 *
 * The `popUpTo` is the whole fix (found on hardware 2026-08-17). A cold tap
 * is not a fresh start: the process was killed, so Android hands
 * `onCreate` a saved instance state and `rememberNavController` faithfully
 * restores the *previous* session's back stack — Now, Status, some other
 * alert — and a plain `navigate` pushes this alert on top of that debris.
 * Four Backs to leave, the first landing on an alert nobody asked for.
 * Popping to Now (never inclusive: it is the start destination and the one
 * thing that must survive) discards the restored entries and makes the
 * stack the same shape whichever way the app was entered.
 *
 * `launchSingleTop` covers the warm re-tap of the alert already on screen,
 * which would otherwise stack a second identical copy of it. */
private fun NavHostController.openAlertFromNotification(alertId: String) {
    navigate(Routes.alertDetail(alertId)) {
        popUpTo(Routes.NOW) { inclusive = false }
        launchSingleTop = true
    }
}

/** A tapped notification whose alert names an item lands that *item*
 * directly on top of Now (ADR-0027) — the same policy
 * [openAlertFromNotification] holds, and deliberately the same body: the
 * cold-tap defect it fixes (#518) is a property of the restored back
 * stack, not of which destination is being pushed onto it, so a second
 * door that skipped the `popUpTo` would regress the fix through the new
 * route. `NavigationStructuralTest` asserts both bodies for that reason. */
private fun NavHostController.openItemFromNotification(itemId: String) {
    navigate(Routes.itemDetail(itemId)) {
        popUpTo(Routes.NOW) { inclusive = false }
        launchSingleTop = true
    }
}

/** Back from a deep-linked destination.
 *
 * The fallback is now unreachable by the notification path:
 * [openAlertFromNotification] pops to Now before pushing, so a deep-linked
 * alert always has Now beneath it and `popBackStack` always succeeds. Kept
 * anyway — it costs one branch, it is the correct answer for any *future*
 * caller that reaches a destination without a stack under it, and the
 * alternative to landing on Now is a blank Activity. */
private fun NavHostController.popBackStackOrHome(home: String) {
    if (!popBackStack()) navigate(home)
}

/** Asks for `POST_NOTIFICATIONS` once per composition of the root, if it is
 * not already granted. minSdk is 35, so this is always a runtime grant and
 * there is no version branch to take.
 *
 * A refusal is not argued with — no rationale dialog, no second ask. The
 * alert lane simply does not ring, and `AlertsScreen`'s health row says so
 * where the user is already looking at alerts. Honesty over reassurance,
 * and it keeps the one thing a permission dialog is for (asking) separate
 * from the thing a nagging dialog does (asking again). */
@Composable
private fun NotificationPermissionRequest() {
    val context = LocalContext.current
    val launcher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { /* Granted or not, the health row is what reports it. */ }

    LaunchedEffect(Unit) {
        val granted = ContextCompat.checkSelfPermission(
            context,
            Manifest.permission.POST_NOTIFICATIONS,
        ) == PackageManager.PERMISSION_GRANTED
        if (!granted) launcher.launch(Manifest.permission.POST_NOTIFICATIONS)
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
// live in `AppRoot` (#514 review), since the cadence must keep running
// while this screen isn't the one on top.
@Composable
private fun ProofScreen(
    facts: CoreFacts?,
    statusLine: String?,
    syncing: Boolean,
    needsToken: Boolean,
    onBack: () -> Unit,
    onSync: () -> Unit,
    /** #535: token entry/forget moved off this screen entirely — Settings
     * is their one home now. A device with no token still needs a way
     * *out* of here to go get one, hence this link rather than a bare
     * "no token" sentence with nothing to do about it. */
    onGoToSettings: () -> Unit,
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
                    Text(
                        "No device token — add one from Settings.",
                        style = MaterialTheme.typography.bodyMedium,
                    )
                } else {
                    SyncButton(syncing = syncing, onSync = onSync)
                }
                TextButton(onClick = onGoToSettings) {
                    Text("Manage device token in Settings")
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

