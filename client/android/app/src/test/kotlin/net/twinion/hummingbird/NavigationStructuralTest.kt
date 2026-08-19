package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// M2 adopted `navigation-compose`, overriding M1's recorded deferral. Two
// things about that adoption are load-bearing and invisible to the
// compiler, so they are gated here — the same no-emulator discipline
// `NowScreenStructuralTest` uses.
class NavigationStructuralTest {

    private val src: String by lazy {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(
            root,
            "client/android/app/src/main/kotlin/net/twinion/hummingbird/MainActivity.kt",
        )
        check(file.isFile) { "MainActivity.kt not found under $root" }
        // Code only. The module doc explains *why* there is no
        // `navDeepLink`, and a gate that scanned raw text would fail on
        // the sentence that documents it.
        file.readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
    }

    @Test
    fun `the foreground sync cadence stays above the NavHost, not inside a destination`() {
        // #514's correction, restated for routes: the cadence must run
        // whichever screen is showing, because Now's mirror is what it
        // keeps fresh and an act taken there is what it flushes. Putting
        // it in a `composable {}` would repeat the original defect —
        // unrefreshed Now and an unflushed act for up to an hour.
        val cadence = src.indexOf("LifecycleResumeEffect(core)")
        val navHost = src.indexOf("NavHost(")
        assertTrue("the resume cadence is missing from MainActivity", cadence >= 0)
        assertTrue("no NavHost found in MainActivity", navHost >= 0)
        assertTrue(
            "the sync cadence must be declared above the NavHost, not within a destination",
            cadence < navHost,
        )
    }

    @Test
    fun `the deep link is read from the intent extra, not from a nav URI`() {
        // Android 12+ bans notification trampolines, so the tap already
        // arrives as an Activity intent (`AlertNotifier`). Reading its
        // extra is the direct expression of what happens; a `navDeepLink`
        // would be a second encoding of the same fact, with its own
        // manifest filter to keep in step.
        assertTrue(
            "MainActivity must read AlertNotifier.EXTRA_ALERT_ID",
            src.contains("AlertNotifier.EXTRA_ALERT_ID"),
        )
        assertTrue(
            "a second tap arrives through onNewIntent while the app is open",
            src.contains("onNewIntent"),
        )
        assertFalse(
            "no navDeepLink — the extra is the one encoding of the deep link",
            src.contains("navDeepLink"),
        )
    }

    @Test
    fun `a notification-opened alert sits directly on top of Now`() {
        // The cold-tap defect (hardware, 2026-08-17): `am kill` means
        // `onCreate` gets a saved instance state, `rememberNavController`
        // restores the killed session's back stack, and a plain `navigate`
        // pushed the alert on top of it — four Backs to leave, the first
        // landing on an unrelated alert. The policy is `now ->
        // alert/{id}`, cold or warm, so the deep link must pop to Now.
        // Both doors, not just the alert one: ADR-0027 added a second
        // notification destination onto the same restored stack, and a
        // door that skipped the pop would regress #518 through the new
        // route while every existing assertion here stayed green.
        for (door in listOf("openAlertFromNotification", "openItemFromNotification")) {
            val open = src.indexOf("fun NavHostController.$door")
            assertTrue("the deep link must go through $door", open >= 0)
            val body = src.substring(open, src.indexOf("\n}", open))
            assertTrue(
                "$door must pop back to Now, not push onto the restored stack",
                body.contains("popUpTo(Routes.NOW)"),
            )
            assertTrue(
                "Now is the start destination and must survive $door's pop",
                body.contains("inclusive = false"),
            )
            assertTrue(
                "a re-tap of what is already on screen must not stack a second copy",
                body.contains("launchSingleTop = true"),
            )
        }

        // And nothing else may reach the detail route from the intent: a
        // bare `navigate` in the collector is exactly the defect.
        val collect = src.indexOf("deepLinkedAlertId.collect")
        assertTrue("the deep-link collector is missing", collect >= 0)
        val collector = src.substring(collect, src.indexOf("\n    }", collect))
        assertFalse(
            "the collector must not navigate to the detail route without popping to Now",
            collector.contains("navigate(Routes.alertDetail"),
        )
        assertFalse(
            "nor to the item route",
            collector.contains("navigate(Routes.itemDetail"),
        )
    }

    @Test
    fun `the M1 showStatus toggle is gone`() {
        // It was the stand-in for exactly this NavHost; leaving both would
        // give Status two ways to be reached and one of them no back stack.
        assertFalse(
            "the showStatus toggle must be replaced by the status route",
            src.contains("showStatus ="),
        )
        for (route in listOf(
            "\"now\"",
            "\"status\"",
            "\"alerts\"",
            "\"alert/{alertId}\"",
            "\"item/{itemId}\"",
        )) {
            assertTrue("route $route is missing", src.contains(route))
        }
    }

    @Test
    fun `the tap destination is the core's answer, never parsed in Kotlin`() {
        // ADR-0027 part 2: `item:` is a key convention with one owner in
        // `hummingbird_domain`. A Kotlin copy would keep compiling after
        // the recipe moved and fail silently — the alert would simply
        // stop opening its item, with nothing failing anywhere.
        assertTrue(
            "the collector must ask the core where a tap leads",
            src.contains("notificationTapTarget("),
        )
        assertFalse(
            "the item id must never be parsed out of the key in Kotlin",
            src.contains("removePrefix(\"item:\")") || src.contains("\"item:\""),
        )
    }

    @Test
    fun `the tap extras decide the destination, never the data URI`() {
        // The URI stays `hummingbird://alert/{id}`, uniqueness-only: it
        // exists so two alerts do not share one PendingIntent. Navigation
        // rides the extras, which is the same one-encoding rule the
        // navDeepLink assertion above holds.
        val notifier = File(
            System.getProperty("hummingbird.repoRoot"),
            "client/android/app/src/main/kotlin/net/twinion/hummingbird/notify/AlertNotifier.kt",
        ).readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
        assertTrue(
            "the tap intent must carry what the alert is about",
            notifier.contains("EXTRA_SOURCE") && notifier.contains("EXTRA_SOURCE_KEY"),
        )
        assertTrue(
            "the data URI stays the alert's, for PendingIntent uniqueness only",
            notifier.contains("hummingbird://alert/"),
        )
        assertFalse(
            "no item URI — the extras decide the destination",
            notifier.contains("hummingbird://item/"),
        )
    }

    @Test
    fun `nav completion (#541) leaves every screen reachable and both notification doors intact`() {
        // #541's own acceptance slice grew this file by design: several
        // slices of nav churn (the bar, the sheet, the ninth screen, the
        // Recall entry) all touch `MainActivity.kt`'s NavHost, and any one
        // of them could quietly break the deep-link doors above.
        // `BottomNavStructuralTest` gates the bar/sheet partition itself in
        // detail; this test restates the two claims #541's own AC makes,
        // together, as a regression net against exactly that churn.
        for (routeConst in listOf("NOW", "TRIAGE", "ALERTS", "STATUS", "DONE", "LEDGER", "RULES", "SETTINGS", "ROUTES")) {
            assertTrue(
                "Routes.$routeConst must have a registered composable — full route reachability",
                src.contains("composable(Routes.$routeConst)"),
            )
        }
        for (door in listOf("openAlertFromNotification", "openItemFromNotification")) {
            assertTrue(
                "$door must still exist — a tapped alert or item deep link must still resolve",
                src.contains("fun NavHostController.$door"),
            )
        }
    }
}
