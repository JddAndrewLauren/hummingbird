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
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.Image
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import kotlin.random.Random
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import net.twinion.hummingbird.speech.DictationHost
import net.twinion.hummingbird.core.CoreHolder
import net.twinion.hummingbird.core.SyncHistoryStore
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

// `NowScreen` (M1-6/#504) is this activity's start destination — the
// frontier, decided core-side and rendered verbatim (`NowScreen.kt`'s own
// doc). The "Status" action behind it used to open the debug `ProofScreen`
// (#141's embedded-core proof pair plus one manual sync); #536 replaces it
// with the real Status screen — the panes shell over
// `hummingbird_core::decisions::panes`, ADR-0017's second ranked-region
// surface — and `ProofScreen` is deleted entirely, its useful affordances
// (token entry/forget, the sync card) having already moved to Settings in
// #535.
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

/** The nine screens' routes, plus Recall's (#541, ADR-0025's map row).
 * Strings, because that is what `NavHost` takes; kept in one place so a
 * typo is a compile error at the use site rather than a silently
 * unreachable screen.
 *
 * All nine top-level screens are reachable now: [NOW]/[TRIAGE]/[ALERTS]/
 * [STATUS] from the bar, [DONE]/[LEDGER]/[RULES]/[SETTINGS]/[ROUTES] from
 * the More sheet (#541 wires the last three's `NavDestination` entries and
 * adds [ROUTES] outright — it had no route at all before this slice).
 * Recall is **not** here at all any more — it is a gesture, not a screen:
 * the search-overlay slice replaced #541/#542's placeholder route with
 * `RecallOverlay`, drawn by `AppRoot` over whatever route is showing (the
 * web `RecallOverlay.tsx`'s own contract), so nothing navigates to it and
 * no route string exists to reach it by. */
private object Routes {
    const val NOW = "now"
    const val STATUS = "status"
    const val ALERTS = "alerts"
    const val RULES = "rules"
    const val TRIAGE = "triage"
    const val DONE = "done"
    const val LEDGER = "ledger"
    const val SETTINGS = "settings"
    const val ROUTES = "routes"
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

/** The bottom nav's one route list (#532) — `nav-bar.ts`'s own rule ported:
 * "a second hand-written list here would silently drop a screen the day one
 * is added". [ON_BAR] and [OVERFLOW] both filter this one enum, so a
 * destination added here lands on the bar or in the More sheet by
 * construction, never neither and never both — `BottomNavStructuralTest`
 * pins both halves against the web's own `nav-bar.ts` for the same reason
 * that file's own test reconstructs `SCREENS` from its two halves.
 *
 * Four on the bar (`ON_THE_BAR` on the web: Now, Triage, Alerts, Status —
 * "the surfaces you *act* on, in the order the day runs", `nav-bar.ts`'s own
 * doc); Done, the Ledger, Rules, Settings and Routes in the sheet — #532
 * landed the first two, #541 the last three, completing all nine
 * screens' reachability. */
private enum class NavDestination(val route: String, val label: String, val onBar: Boolean) {
    NOW(Routes.NOW, "Now", onBar = true),
    TRIAGE(Routes.TRIAGE, "Triage", onBar = true),
    ALERTS(Routes.ALERTS, "Alerts", onBar = true),
    STATUS(Routes.STATUS, "Status", onBar = true),
    DONE(Routes.DONE, "Done", onBar = false),
    LEDGER(Routes.LEDGER, "Ledger", onBar = false),
    RULES(Routes.RULES, "Rules", onBar = false),
    SETTINGS(Routes.SETTINGS, "Settings", onBar = false),
    ROUTES(Routes.ROUTES, "Routes", onBar = false),
    ;

    companion object {
        val ON_BAR: List<NavDestination> = entries.filter { it.onBar }
        val OVERFLOW: List<NavDestination> = entries.filterNot { it.onBar }
    }
}

/** Each destination's glyph — the web's `screen-icons.ts` map, ported onto
 * the vendored Lucide drawables (`res/drawable/ic_*.xml`, each header
 * naming its source glyph). A `when` with no `else` rather than an enum
 * parameter: exhaustiveness makes a tenth destination fail to compile until
 * it names a glyph, and `BottomNavStructuralTest`'s enum parser keeps its
 * three-argument constructor shape untouched. */
private fun navIcon(destination: NavDestination): Int = when (destination) {
    NavDestination.NOW -> R.drawable.ic_zap
    NavDestination.TRIAGE -> R.drawable.ic_inbox
    NavDestination.ALERTS -> R.drawable.ic_alert
    NavDestination.STATUS -> R.drawable.ic_activity
    NavDestination.DONE -> R.drawable.ic_circle_check
    NavDestination.LEDGER -> R.drawable.ic_scroll_text
    NavDestination.RULES -> R.drawable.ic_siren
    NavDestination.SETTINGS -> R.drawable.ic_settings
    NavDestination.ROUTES -> R.drawable.ic_route
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
/** The least time the pull-to-refresh flag stays up — see `refresh()`'s
 * own comment for the two reasons (an M3 indicator race, and legibility). */
private const val MIN_REFRESH_VISIBLE_MS = 600L

/** How far a scroll runs one way before the chrome hides (48px) or
 * re-shows (16px) — hide reluctantly, reveal eagerly, Gmail's own feel. */
private const val CHROME_HIDE_THRESHOLD_PX = 48f
private const val CHROME_SHOW_THRESHOLD_PX = 16f

/** Gmail-style chrome hiding: scrolling down hides the top bar and the
 * bottom bar and collapses the Capture FAB to its round form; scrolling up
 * brings them back. One boolean flipped by a delta accumulator — never an
 * offset the bars track per-frame, because collapsing the slot's measured
 * height (AnimatedVisibility below) is what lets the Scaffold's padding
 * shrink and the content reclaim the space.
 *
 * The connection reads `consumed.y`, never `available.y` — the whole
 * pull-to-refresh interplay: overscrolling down at the top of a list
 * consumes nothing, and what `PullToRefreshBox` itself consumes during a
 * pull is downward, so a pull can only ever move the accumulator toward
 * "show". The two nested-scroll owners compose without knowing about each
 * other. A direction flip resets the run so small reversals never jitter
 * the bars; fling frames arrive through `onPostScroll` too, so momentum
 * behaves without an `onPostFling` arm. */
@Stable
private class ChromeScrollState {
    var chromeVisible by mutableStateOf(true)
        private set
    private var accumulated = 0f

    fun reveal() {
        chromeVisible = true
        accumulated = 0f
    }

    val connection = object : NestedScrollConnection {
        override fun onPostScroll(
            consumed: Offset,
            available: Offset,
            source: NestedScrollSource,
        ): Offset {
            val dy = consumed.y
            if (dy == 0f) return Offset.Zero
            accumulated = if ((dy < 0f) != (accumulated < 0f) && accumulated != 0f) dy else accumulated + dy
            if (accumulated < -CHROME_HIDE_THRESHOLD_PX) {
                chromeVisible = false
                accumulated = 0f
            } else if (accumulated > CHROME_SHOW_THRESHOLD_PX) {
                chromeVisible = true
                accumulated = 0f
            }
            return Offset.Zero
        }
    }
}

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
        val nowMs = System.currentTimeMillis()
        val outcome = host.run(
            nowMs,
            trigger,
            false,
            Random.nextDouble(),
        )
        val credentialEvent =
            host.takeEvents().any { it.kind == "credential_needed" }
        needsToken = credentialEvent ||
            outcome.kind == "no_credential" || outcome.kind == "held"
        if (isInformativeSyncOutcome(outcome.kind)) {
            lastSyncOutcomeKind = outcome.kind
            lastSyncAtMs = nowMs
            // #536: the reachability pane's own durable copy — see
            // `SyncHistoryStore`'s header for why this lives here rather
            // than in `hummingbird-core`.
            SyncHistoryStore.recordInformative(context, outcome.kind, nowMs)
        }
        syncTick += 1
    }

    // Pull-to-refresh's in-flight flag: the gesture is only a second door
    // onto the one `sync("user")` cadence above — never a screen-local
    // cycle — and the indicator spins for exactly the cycle's duration;
    // the repaint itself still arrives through `syncTick`. Re-entry is
    // dropped rather than queued: a second pull mid-cycle has nothing to
    // add that the in-flight cycle won't already deliver.
    var refreshing by remember { mutableStateOf(false) }
    fun refresh() {
        if (refreshing) return
        scope.launch {
            refreshing = true
            try {
                // Hold the flag for a visible beat even when the cycle
                // answers instantly (a tokenless device's "held" comes back
                // in ~0ms): material3's PullToRefreshBox strands its
                // indicator at the threshold when isRefreshing flips
                // true->false inside its own settle animation (sighted on
                // the Fold AVD, 2026-08-20 device pass), and a sub-frame
                // flash would also read as "nothing happened" to a human.
                val startedMs = System.currentTimeMillis()
                sync("user")
                val elapsedMs = System.currentTimeMillis() - startedMs
                if (elapsedMs < MIN_REFRESH_VISIBLE_MS) {
                    delay(MIN_REFRESH_VISIBLE_MS - elapsedMs)
                }
            } finally {
                refreshing = false
            }
        }
    }

    LaunchedEffect(Unit) {
        val host = CoreHolder.get(context)
        core = host
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

    // The token entry/forget gestures — `SettingsScreen`'s own widgets
    // (moved off the debug `ProofScreen` entirely in #535, `ProofScreen`
    // itself deleted in #536). Hoisted here rather than inlined in its
    // `composable {}` body, the same reason `sync` above is: `AppRoot` is
    // the one place that reaches both `core` and `needsToken`, whichever
    // route is on screen, and a screen-scoped copy would have to re-derive
    // state `AppRoot` already tracks.
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
    }

    // The bottom nav's own state (#532): which sheet, if any, is open, and
    // the live route — read from the back stack rather than held as a
    // second copy, so a destination reached any other way (a notification
    // deep link) still lights up the right tab.
    // The search overlay (the search-overlay slice): a gesture, not a route —
    // the web RecallOverlay's own contract, so it draws OVER the current
    // screen and Back closes it without navigating. Saveable: an Activity
    // recreation mid-search brings the overlay back.
    var recallOpen by rememberSaveable { mutableStateOf(false) }
    var moreSheetOpen by remember { mutableStateOf(false) }
    // `rememberSaveable`, unlike the More sheet's flag: a fold/unfold
    // mid-capture recreates the Activity, and the sheet must come back
    // standing over its surviving draft (the draft itself lives in
    // `CaptureViewModel`'s store) — the More sheet holds nothing, so
    // losing it to recreation costs one tap and no words.
    var captureSheetOpen by rememberSaveable { mutableStateOf(false) }
    val currentRoute = navController.currentBackStackEntryAsState().value?.destination?.route
    val chrome = remember { ChromeScrollState() }
    // Whatever a scroll hid, a navigation reveals: a fresh route starts at
    // its own top, and arriving with no chrome would leave no way to know
    // where you are.
    LaunchedEffect(currentRoute) { chrome.reveal() }

    // A bar or More-sheet tap: `popUpTo` + `saveState` + `restoreState` is
    // the standard bottom-nav idiom — each tab keeps its own back stack
    // across switches instead of stacking a new copy of Now underneath
    // every visit, and `launchSingleTop` covers a re-tap of the tab already
    // open. Every route with a `NavDestination` entry is reached through
    // this door, including cross-surface links (#574); the plain `navigate`
    // calls below all push detail/takeover routes, which are poppable by
    // design and carry no tab.
    fun goToTab(route: String) {
        moreSheetOpen = false
        navController.navigate(route) {
            popUpTo(Routes.NOW) { saveState = true }
            launchSingleTop = true
            restoreState = true
        }
    }

    Box {
        Scaffold(
            // The one hook for chrome hiding: every scrollable in every screen
            // dispatches through the tree to this ancestor, so both LazyColumn
            // screens and verticalScroll screens drive the same accumulator.
            modifier = Modifier.nestedScroll(chrome.connection),
            topBar = {
                // The bottom bar's own visibility rule, applied above: chrome
                // belongs to the top-level surfaces, and a detail/takeover
                // route draws none of it.
                if (NavDestination.entries.any { it.route == currentRoute }) {
                    // Height-collapse, not offset: a 0-height slot shrinks the
                    // Scaffold's padding, the content reclaims the space, and
                    // the still-unconsumed status-bar inset falls through to
                    // each screen's own inner Scaffold (#615's arrangement,
                    // untouched) — no dead band, no content under the clock.
                    AnimatedVisibility(
                        visible = chrome.chromeVisible,
                        enter = expandVertically() + fadeIn(),
                        exit = shrinkVertically() + fadeOut(),
                    ) {
                        AppTopBar(
                            dark = resolveDarkTheme(themePreference, isSystemInDarkTheme()),
                            onSearch = { recallOpen = true },
                        )
                    }
                }
            },
            bottomBar = {
                // Hidden on a route with no [NavDestination] entry — a
                // detail/takeover route (item, alert, Grill) or the Recall
                // placeholder, which is deliberately not one either (its own
                // doc, above) — the same "not every screen carries the bar"
                // the web's shell holds by mounting exactly one nav form.
                if (NavDestination.entries.any { it.route == currentRoute }) {
                    AnimatedVisibility(
                        visible = chrome.chromeVisible,
                        enter = expandVertically(expandFrom = Alignment.Bottom) + fadeIn(),
                        exit = shrinkVertically(shrinkTowards = Alignment.Bottom) + fadeOut(),
                    ) {
                        BottomNavBar(
                            currentRoute = currentRoute,
                            onNavigate = ::goToTab,
                            onMore = { moreSheetOpen = true },
                        )
                    }
                }
            },
            floatingActionButton = {
                // The bar's own visibility condition: capture is the app's
                // global primary action on every top-level surface, and a
                // detail/takeover route is a task mid-flight that a floating
                // "Capture" would talk over. The extended-FAB shape, wording
                // and fill are the design kit's own (`ui_kits/android/`, its
                // `Fab`): feather at 20dp, the word "Capture", 20dp corners —
                // and the one place brand orange appears as a large fill on
                // Android (`colorScheme.primary` is the ember accent).
                if (NavDestination.entries.any { it.route == currentRoute }) {
                    ExtendedFloatingActionButton(
                        onClick = { captureSheetOpen = true },
                        // Gmail's own collapse: scrolled-away chrome shrinks the
                        // extended FAB to its round icon form; the FAB itself
                        // stays — capture must survive a reading scroll.
                        expanded = chrome.chromeVisible,
                        containerColor = MaterialTheme.colorScheme.primary,
                        contentColor = MaterialTheme.colorScheme.onPrimary,
                        shape = RoundedCornerShape(20.dp),
                        icon = {
                            Icon(
                                painterResource(R.drawable.ic_feather),
                                contentDescription = null,
                                modifier = Modifier.size(20.dp),
                            )
                        },
                        text = { Text("Capture") },
                    )
                }
            },
        ) { padding ->
            NavHost(
                navController = navController,
                startDestination = Routes.NOW,
                // Consumed as well as applied: padding alone leaves the
                // status-bar inset unconsumed, so every tab screen's own nested
                // bare `Scaffold` re-applies it — a blank band between the top
                // bar and each screen's content.
                modifier = Modifier
                    .padding(padding)
                    .consumeWindowInsets(padding),
            ) {
                composable(Routes.NOW) {
                    NowScreen(
                        syncTick = syncTick,
                        isRefreshing = refreshing,
                        onRefresh = ::refresh,
                        // A tapped card expands in place (NowScreen's own
                        // ItemDetailPanel item) — Grill is the one gesture that
                        // still leaves the screen, and Back from the takeover
                        // lands on Now with the panel still standing, since the
                        // selection lives in the Activity-scoped NowViewModel.
                        onGrill = { itemId -> navController.navigate(Routes.grill(itemId, "detail")) },
                        // The unbound panes' setup door — through `goToTab`,
                        // never a plain `navigate` (#574's own reasoning on
                        // the Status screen's identical door).
                        onGoToSettings = { goToTab(Routes.SETTINGS) },
                    )
                }
                composable(Routes.STATUS) {
                    StatusScreen(
                        syncTick = syncTick,
                        isRefreshing = refreshing,
                        onRefresh = ::refresh,
                        // Through `goToTab`, never a plain `navigate` (#574):
                        // Settings is a More destination, so it must land in the
                        // More stack wherever it was entered from. A plain
                        // `navigate` here made Settings the Status tab's top
                        // entry, which `restoreState` then faithfully restored on
                        // every later Status tap — the tab was unreachable from
                        // its own bar button.
                        onGoToSettings = { goToTab(Routes.SETTINGS) },
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
                        isRefreshing = refreshing,
                        onRefresh = ::refresh,
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
                        isRefreshing = refreshing,
                        onRefresh = ::refresh,
                        onGrill = { itemId -> navController.navigate(Routes.grill(itemId, "triage")) },
                    )
                }
                composable(Routes.DONE) {
                    DoneScreen(
                        syncTick = syncTick,
                        onBack = { navController.popBackStackOrHome(Routes.NOW) },
                    )
                }
                composable(Routes.LEDGER) {
                    LedgerScreen(
                        syncTick = syncTick,
                        onBack = { navController.popBackStackOrHome(Routes.NOW) },
                    )
                }
                composable(Routes.ROUTES) {
                    RoutesScreen(onBack = { navController.popBackStackOrHome(Routes.NOW) })
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

        if (recallOpen) {
            RecallOverlay(
                syncTick = syncTick,
                onClose = { recallOpen = false },
                // Grill is the one gesture that leaves the overlay: close it
                // first, or the takeover would open underneath.
                onGrill = { itemId ->
                    recallOpen = false
                    navController.navigate(Routes.grill(itemId, "detail"))
                },
            )
        }
    }

    if (moreSheetOpen) {
        MoreSheet(
            currentRoute = currentRoute,
            onNavigate = ::goToTab,
            onSearch = {
                moreSheetOpen = false
                recallOpen = true
            },
            onDismiss = { moreSheetOpen = false },
        )
    }

    if (captureSheetOpen) {
        // The sheet's dictation host lives exactly as long as the sheet is
        // composed: `SpeechRecognizer` is a Context-bound platform session
        // with a `destroy()` lifecycle, and where `CaptureActivity` owns
        // its host in `onCreate`/`onDestroy`, this Activity hosts the
        // sheet as a composition — so the composition is the lifetime, and
        // no recognizer session outlives the surface that could use it.
        val dictation = remember { DictationHost(context) }
        DisposableEffect(Unit) {
            onDispose { dictation.destroy() }
        }
        CaptureSheet(
            startListening = dictation::startListening,
            onDismiss = { captureSheetOpen = false },
            // A user-attributed cycle, not a wait for the 60-second timer
            // leg: the capture is already durable locally
            // (`CaptureViewModel.submit`'s local-first contract), so this
            // only hurries the mirror — `syncTick` bumps when it lands and
            // Now/Triage re-read.
            onCaptured = { scope.launch { sync("user") } },
        )
    }
}

/** The design kit's Android `TopBar` (`ui_kits/android/AndroidScreens.jsx`):
 * the app icon at 24dp on its 22.37% squircle plate, the lowercase wordmark
 * in the display face, and the Recall trigger trailing under the name every
 * web trigger shares ("Search everything" — `Header.tsx`, `NavRail.tsx`,
 * `NavBar.tsx`). The icon is `Image`, never `Icon`: the plate is part of
 * the artwork and must not be tinted, and the light/dark plates are two
 * separate exports swapped with the resolved theme, the same way
 * `NavRail.tsx` swaps its `srcSet`. */
@Composable
private fun AppTopBar(
    dark: Boolean,
    onSearch: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .statusBarsPadding()
            .padding(horizontal = 24.dp, vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Image(
            painterResource(if (dark) R.drawable.app_icon_dark else R.drawable.app_icon_light),
            contentDescription = null,
            modifier = Modifier
                .size(24.dp)
                .clip(RoundedCornerShape(percent = 22)),
        )
        Text(
            "hummingbird",
            style = MaterialTheme.typography.headlineSmall.copy(
                fontSize = 20.sp,
                letterSpacing = (-0.02).em,
            ),
            modifier = Modifier.weight(1f),
        )
        IconButton(onClick = onSearch) {
            Icon(
                painterResource(R.drawable.ic_search),
                contentDescription = "Search everything",
                modifier = Modifier.size(20.dp),
            )
        }
    }
}

/** The four bar destinations plus a fifth "More" control — `nav-bar.ts`'s
 * phone form, ported: a 20dp glyph above the label, both always visible
 * (`NavBar.tsx`'s own layout). `NavigationBarItem` takes no accessible-name
 * parameter beyond its own `label`, which is what Compose announces for it,
 * the same "one visible name is the accessible one" rule the web's own
 * `aria-label` follows — so every icon slot passes `contentDescription =
 * null`: the label is the name, and a described icon would announce twice.
 *
 * "More" reads as selected whenever the open screen is one it hides
 * (`NAV_BAR_OVERFLOW` on the web) — the identical "you are nowhere"
 * correction `nav-bar.ts`'s own `isOverflowScreen` documents. */
@Composable
private fun BottomNavBar(
    currentRoute: String?,
    onNavigate: (String) -> Unit,
    onMore: () -> Unit,
) {
    val overflowActive = NavDestination.OVERFLOW.any { it.route == currentRoute }
    NavigationBar {
        for (destination in NavDestination.ON_BAR) {
            NavigationBarItem(
                selected = destination.route == currentRoute,
                onClick = { onNavigate(destination.route) },
                icon = {
                    Icon(
                        painterResource(navIcon(destination)),
                        contentDescription = null,
                        modifier = Modifier.size(20.dp),
                    )
                },
                label = { Text(destination.label) },
                alwaysShowLabel = true,
            )
        }
        NavigationBarItem(
            selected = overflowActive,
            onClick = onMore,
            icon = {
                Icon(
                    painterResource(R.drawable.ic_ellipsis),
                    contentDescription = null,
                    modifier = Modifier.size(20.dp),
                )
            },
            label = { Text("More") },
            alwaysShowLabel = true,
        )
    }
}

/** The sheet the bar's "More" control opens: the destinations the bar
 * cannot hold — Done, the Ledger, Rules, Settings and Routes (#532, #541) —
 * plus, last, the Recall entry point (#541): a gesture, not a destination,
 * so it is drawn separately from the [NavDestination.OVERFLOW] loop rather
 * than folded into it, the same separation `nav-bar.ts`'s own `onSearch` row
 * holds on the web. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun MoreSheet(
    currentRoute: String?,
    onNavigate: (String) -> Unit,
    onSearch: () -> Unit,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 24.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                "More",
                style = MaterialTheme.typography.headlineSmall,
                modifier = Modifier.padding(bottom = 12.dp),
            )
            for (destination in NavDestination.OVERFLOW) {
                MoreSheetRow(
                    label = destination.label,
                    iconRes = navIcon(destination),
                    active = destination.route == currentRoute,
                    onClick = { onNavigate(destination.route) },
                )
            }
            MoreSheetRow(
                label = "Search everything",
                iconRes = R.drawable.ic_search,
                // A gesture has no route to be "current" on — the overlay
                // closes this sheet as it opens.
                active = false,
                onClick = onSearch,
            )
        }
    }
}

/** One More-sheet row: a 20dp glyph beside the label — `NavBar.tsx`'s own
 * sheet rows, ported. The icon takes the row's text colour (`tint` defaults
 * to `LocalContentColor`, set here on both children): icons never carry
 * colour independently of their label. */
@Composable
private fun MoreSheetRow(
    label: String,
    iconRes: Int,
    active: Boolean,
    onClick: () -> Unit,
) {
    val color = if (active) {
        MaterialTheme.colorScheme.primary
    } else {
        MaterialTheme.colorScheme.onSurface
    }
    TextButton(onClick = onClick) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                painterResource(iconRes),
                contentDescription = null,
                modifier = Modifier.size(20.dp),
                tint = color,
            )
            Text(label, color = color)
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
