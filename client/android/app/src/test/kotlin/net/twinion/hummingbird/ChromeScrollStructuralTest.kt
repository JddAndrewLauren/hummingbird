package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// Gmail-style chrome hiding (operator request 2026-08-20): scroll down and
// the top bar, bottom bar hide while the Capture FAB collapses; scroll up
// and they return. The mechanism's load-bearing choices are exactly the
// kind no render test can reach:
//
// - The connection accumulates `consumed.y`, never `available.y` — the
//   pull-to-refresh interplay: top-of-list overscroll consumes nothing, so
//   a pull that only ever pulls cannot hide the chrome. (Not a total
//   guarantee — see `ChromeScrollState`'s own doc on the cancelled pull.)
//
// The accumulator's arithmetic — thresholds, the direction-flip reset,
// `reveal()` — is `ChromeScrollStateTest`'s, not this file's.
// - Height-collapse via AnimatedVisibility, not an offset — a 0-height
//   slot shrinks the Scaffold's padding so content reclaims the space, and
//   the status-bar inset falls through to each screen's inner Scaffold.
//   That hand-off only works while the NavHost keeps #615's
//   `.consumeWindowInsets(padding)`, so that pin lives here too.
class ChromeScrollStructuralTest {

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
    fun `the chrome connection hangs on the one Scaffold and reads consumed, never available`() {
        val src = source("MainActivity.kt")
        assertTrue(
            "the Scaffold carries the nested-scroll hook",
            src.contains("modifier = Modifier.nestedScroll(chrome.connection)"),
        )
        assertTrue(
            "the accumulator reads what the child actually scrolled",
            src.contains("val dy = consumed.y"),
        )
        assertFalse(
            "keying on available.y would let a top-of-list pull hide the chrome",
            src.contains("val dy = available.y"),
        )
    }

    @Test
    fun `both bars hide by height-collapse inside their slots`() {
        val src = source("MainActivity.kt")
        val topBarSlot = src.substringAfter("topBar = {").substringBefore("bottomBar = {")
        assertTrue(
            "the top bar hides by AnimatedVisibility height-collapse",
            topBarSlot.contains("AnimatedVisibility("),
        )
        val bottomBarSlot = src.substringAfter("bottomBar = {").substringBefore("floatingActionButton = {")
        assertTrue(
            "the bottom bar hides by AnimatedVisibility height-collapse",
            bottomBarSlot.contains("AnimatedVisibility("),
        )
    }

    @Test
    fun `the FAB collapses with the chrome rather than hiding`() {
        val src = source("MainActivity.kt")
        assertTrue(
            "the extended FAB's expanded form follows the chrome state",
            src.contains("expanded = chrome.chromeVisible"),
        )
    }

    @Test
    fun `navigation reveals whatever a scroll hid`() {
        val src = source("MainActivity.kt")
        assertTrue(
            "a route change resets the chrome to visible",
            src.contains("LaunchedEffect(currentRoute) { chrome.reveal() }"),
        )
    }

    @Test
    fun `the NavHost still consumes the Scaffold padding's insets`() {
        val src = source("MainActivity.kt")
        assertTrue(
            "#615's inset guard survives — remove this and hidden chrome puts content under the status bar",
            src.contains(".consumeWindowInsets(padding)"),
        )
    }
}
