package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The M2 counterpart of `NowScreenStructuralTest`, and the gate ADR-0014
// most needs: the alerts surfaces must apply no liveness predicate and no
// ordering of their own.
//
// `AlertRecord` ships `isLive` and `canAck` as *decided answers*
// (`ffi-mobile/src/lib.rs`). A Kotlin `dismissedAt == null` test is the
// exact bug the three-clause predicate exists to prevent — it cannot tell
// an expired-then-re-raised occurrence from an acked one, and `expires_at`
// is never written back as a dismissal — and it would compile, run and look
// right on every fixture anyone would think to write. Only a source gate
// catches it, which is why this test reads the files.
class AlertsScreenStructuralTest {

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }

    /** The file's *code*, with comments removed. A doc comment must be
     * free to name the thing it forbids — "must not read `dismissedAt`" is
     * the clearest way to say it — so a gate that scanned raw text would
     * punish the documentation it depends on. Block comments and
     * whole-line `//` comments both go. */
    private fun source(name: String) =
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/$name")
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")

    private val alertsScreenSrc by lazy { source("AlertsScreen.kt") }
    private val alertsViewModelSrc by lazy { source("AlertsViewModel.kt") }
    private val detailScreenSrc by lazy { source("AlertDetailScreen.kt") }
    private val detailViewModelSrc by lazy { source("AlertDetailViewModel.kt") }

    private val allFour by lazy {
        listOf(
            "AlertsScreen.kt" to alertsScreenSrc,
            "AlertsViewModel.kt" to alertsViewModelSrc,
            "AlertDetailScreen.kt" to detailScreenSrc,
            "AlertDetailViewModel.kt" to detailViewModelSrc,
        )
    }

    @Test
    fun `no alert surface ever reads dismissedAt`() {
        for ((name, src) in allFour) {
            assertFalse(
                "$name must not read dismissedAt — isLive and canAck are the decided answers",
                src.contains("dismissedAt"),
            )
        }
    }

    @Test
    fun `no alert surface re-derives liveness from the timestamp columns`() {
        // The other two clauses of `Alert::is_live`, for the same reason.
        for ((name, src) in allFour) {
            assertFalse(
                "$name must not test resolvedAt itself",
                Regex("""resolvedAt\s*(==|!=|<|>)""").containsMatchIn(src),
            )
            assertFalse(
                "$name must not test expiresAt itself",
                Regex("""expiresAt\s*(==|!=|<|>)""").containsMatchIn(src),
            )
        }
    }

    @Test
    fun `no alert surface sorts the list itself`() {
        // `Core::live_alerts` sorts (`raised_at` desc, id tiebreak). A
        // second comparator here would silently disagree the first time
        // the core's own ordering changed.
        for ((name, src) in allFour) {
            assertFalse(
                "$name must not implement its own comparator",
                src.contains("sortedBy") || src.contains("sortWith") || src.contains(".sorted("),
            )
        }
    }

    @Test
    fun `canAck gates the Ack affordance on both surfaces`() {
        for ((name, src) in listOf(
            "AlertsScreen.kt" to alertsScreenSrc,
            "AlertDetailScreen.kt" to detailScreenSrc,
        )) {
            assertTrue(
                "$name must gate the Ack button on record.canAck",
                src.contains("canAck"),
            )
        }
    }

    @Test
    fun `an acked row dims to the web card's own value`() {
        assertTrue(
            "AlertsScreen must carry AlertCard.tsx's 0.55 acked opacity",
            alertsScreenSrc.contains("0.55f"),
        )
    }

    @Test
    fun `both production factories wire the real MobileTaskHost, not a fake`() {
        for ((name, src) in listOf(
            "AlertsViewModel.kt" to alertsViewModelSrc,
            "AlertDetailViewModel.kt" to detailViewModelSrc,
        )) {
            val factory = Regex("""fun create\(context: Context\)[\s\S]*?\n {4}}""")
                .find(src)
                ?.value
                ?: error("could not locate $name's create(context)")
            assertTrue(
                "$name.create must reach CoreHolder.get(context.applicationContext)",
                factory.contains("CoreHolder.get(context.applicationContext)"),
            )
        }
    }

    @Test
    fun `the view models call the real seam reads`() {
        assertTrue(
            "AlertsViewModel must close over MobileTaskHost.alerts",
            alertsViewModelSrc.contains(".alerts("),
        )
        assertTrue(
            "AlertsViewModel must close over MobileTaskHost.ackAlert",
            alertsViewModelSrc.contains(".ackAlert("),
        )
        assertTrue(
            "AlertDetailViewModel must close over MobileTaskHost.alert",
            detailViewModelSrc.contains(".alert("),
        )
        assertTrue(
            "AlertDetailViewModel must close over MobileTaskHost.ackAlert",
            detailViewModelSrc.contains(".ackAlert("),
        )
    }

    @Test
    fun `the health rows report both silent-failure conditions`() {
        // Neither condition raises an error anywhere else: a notification
        // posted with notifications disabled simply vanishes, and DND
        // bypass is inert without policy access. This screen is the only
        // place either becomes visible.
        assertTrue(
            "AlertsScreen must check areNotificationsEnabled",
            alertsScreenSrc.contains("areNotificationsEnabled()"),
        )
        assertTrue(
            "AlertsScreen must check isNotificationPolicyAccessGranted",
            alertsScreenSrc.contains("isNotificationPolicyAccessGranted"),
        )
    }
}
