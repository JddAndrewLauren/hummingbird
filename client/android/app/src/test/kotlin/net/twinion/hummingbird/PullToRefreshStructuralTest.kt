package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// Pull-to-refresh on the four bar tabs, and the titles those tabs no longer
// draw. Two contracts, both invisible to a render test:
//
// 1. The pull gesture is a second door onto `AppRoot`'s ONE sync cadence —
//    `refresh()` wraps `sync("user")` and nothing else. A screen that grew
//    its own `MobileTaskHost.run(...)` would sync without recording
//    `SyncHistoryStore`, without bumping `syncTick`, and without the
//    credential check — the exact split #514 removed.
// 2. A bar tab draws no headline: the bottom bar already names it, so the
//    title was duplication (operator request 2026-08-20). Overflow screens
//    (Done/Ledger/Rules/Settings) keep theirs — only "More" lights for
//    them, so the headline is their sole on-screen identity.
class PullToRefreshStructuralTest {

    private val tabScreens = listOf(
        "NowScreen.kt" to "Now",
        "TriageScreen.kt" to "Triage",
        "AlertsScreen.kt" to "Alerts",
        "StatusScreen.kt" to "Status",
    )

    private fun source(name: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, "client/android/app/src/main/kotlin/net/twinion/hummingbird/$name")
        check(file.isFile) { "$name not found under $root" }
        return file.readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
    }

    @Test
    fun `every bar tab wraps its content in PullToRefreshBox and takes the refresh pair`() {
        for ((name, _) in tabScreens) {
            val src = source(name)
            assertTrue(
                "$name must wrap its Scaffold content in PullToRefreshBox",
                src.contains("PullToRefreshBox("),
            )
            assertTrue(
                "$name must take the in-flight flag as a parameter, not own one",
                src.contains("isRefreshing: Boolean"),
            )
            assertTrue(
                "$name must take the refresh door as a parameter",
                src.contains("onRefresh: () -> Unit"),
            )
        }
    }

    @Test
    fun `refresh is AppRoot's wrapper over the one sync door, in-flight for the cycle's duration`() {
        val src = source("MainActivity.kt")
        assertTrue(
            "AppRoot owns the refresh wrapper",
            src.contains("fun refresh()"),
        )
        assertTrue(
            "refresh goes through the one sync door as a user trigger",
            src.contains("sync(\"user\")"),
        )
        assertTrue(
            "the in-flight flag drops even when the cycle throws",
            src.contains("} finally {"),
        )
        for ((name, _) in tabScreens) {
            val screen = source(name)
            assertFalse(
                "$name must not reach the sync seam itself — the refresh door is AppRoot's",
                screen.contains("MobileTaskHost"),
            )
        }
    }

    @Test
    fun `a bar tab draws no headline of its own`() {
        for ((name, title) in tabScreens) {
            val src = source(name)
            assertFalse(
                "$name must not draw the \"$title\" headline the bottom bar already provides",
                src.contains("Text(\"$title\""),
            )
            assertFalse(
                "$name must not reintroduce a headlineLarge title",
                src.contains("headlineLarge"),
            )
        }
    }

    @Test
    fun `Triage's counts survive the header removal, still the record's own fields`() {
        val src = source("TriageScreen.kt")
        assertTrue(
            "the captured count is the record's own field",
            src.contains("board.capturedCount"),
        )
        assertTrue(
            "the grilling count is the record's own field",
            src.contains("board.grillingCount"),
        )
    }
}
