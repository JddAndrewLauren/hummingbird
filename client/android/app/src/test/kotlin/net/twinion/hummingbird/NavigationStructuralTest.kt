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
        val open = src.indexOf("fun NavHostController.openAlertFromNotification")
        assertTrue(
            "the deep link must go through openAlertFromNotification",
            open >= 0,
        )
        val body = src.substring(open, src.indexOf("\n}", open))
        assertTrue(
            "the deep link must pop back to Now, not push onto the restored stack",
            body.contains("popUpTo(Routes.NOW)"),
        )
        assertTrue(
            "Now is the start destination and must survive the pop",
            body.contains("inclusive = false"),
        )
        assertTrue(
            "a re-tap of the alert already on screen must not stack a second copy",
            body.contains("launchSingleTop = true"),
        )

        // And nothing else may reach the detail route from the intent: a
        // bare `navigate` in the collector is exactly the defect.
        val collect = src.indexOf("deepLinkedAlertId.collect")
        assertTrue("the deep-link collector is missing", collect >= 0)
        val collector = src.substring(collect, src.indexOf("\n    }", collect))
        assertFalse(
            "the collector must not navigate to the detail route without popping to Now",
            collector.contains("navigate(Routes.alertDetail"),
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
        for (route in listOf("\"now\"", "\"status\"", "\"alerts\"", "\"alert/{alertId}\"")) {
            assertTrue("route $route is missing", src.contains(route))
        }
    }
}
