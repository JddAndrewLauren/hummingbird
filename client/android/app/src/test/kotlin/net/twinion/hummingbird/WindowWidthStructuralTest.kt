package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The unfolded slice's shell contract: ONE breakpoint, shared with the web,
// and exactly one nav form mounted per side of it. The breakpoint pin is
// the `BottomNavStructuralTest` idiom pointed at `breakpoints.ts`; the nav
// pins are source-structural because no JVM render can mount two window
// widths at once — `AdaptiveGridWidthTest` is the half that measures.
class WindowWidthStructuralTest {

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }

    private fun source(name: String) =
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/$name")
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")

    private val mainActivitySrc by lazy { source("MainActivity.kt") }
    private val navRailSrc by lazy { source("NavRail.kt") }
    private val windowWidthSrc by lazy { source("ui/WindowWidth.kt") }

    @Test
    fun `the breakpoint is the web's own, dp-for-px`() {
        val web = repoFile("client/web/src/shell/breakpoints.ts")
        val webValue = Regex("""export const PHONE_MAX_WIDTH_PX = (\d+);""")
            .find(web)
            ?.groupValues?.get(1)?.toInt()
            ?: error("could not find PHONE_MAX_WIDTH_PX in breakpoints.ts")
        assertTrue(
            "ui/WindowWidth.kt must carry the web's breakpoint ($webValue) — if the web " +
                "moved its one breakpoint, move this one with it",
            windowWidthSrc.contains("const val PHONE_MAX_WIDTH_DP = $webValue"),
        )
    }

    @Test
    fun `the rail mounts only on a wide window, on the bar's own route rule`() {
        assertTrue(
            "the rail must be gated on wide AND on the top-level-route rule the bar uses",
            mainActivitySrc.contains(
                "if (wide && NavDestination.entries.any { it.route == currentRoute }) {",
            ),
        )
        assertTrue(
            "the rail sits beside the Scaffold, which takes the Row's remaining width",
            mainActivitySrc.contains("HbNavRail(currentRoute = currentRoute, onNavigate = ::goToTab)"),
        )
    }

    @Test
    fun `the bottom bar is gated off a wide window, so exactly one nav form mounts`() {
        val bottomBarSlot = mainActivitySrc
            .substringAfter("bottomBar = {")
            .substringBefore("floatingActionButton = {")
        assertTrue(
            "the bottomBar slot's condition must carry !wide",
            bottomBarSlot.contains("if (!wide && NavDestination.entries.any { it.route == currentRoute }) {"),
        )
    }

    @Test
    fun `the rail carries every destination and no More item`() {
        assertTrue(
            "the rail must iterate the whole NavDestination enum — all nine screens, " +
                "web-rail parity — never the bar's ON_BAR partition",
            navRailSrc.contains("for (destination in NavDestination.entries)"),
        )
        assertFalse(
            "no More item on the rail — the More sheet is phone-only by construction",
            navRailSrc.contains("\"More\"") || navRailSrc.contains("onMore"),
        )
        assertFalse(
            "the rail must not reach the ON_BAR/OVERFLOW partition at all",
            navRailSrc.contains("ON_BAR") || navRailSrc.contains("OVERFLOW"),
        )
    }

    @Test
    fun `the rail never hides with the chrome`() {
        // The bars hide by AnimatedVisibility inside their Scaffold slots;
        // the rail is outside the Scaffold and costs the content no height,
        // so it stays — the web rail is always visible too.
        assertFalse(
            "NavRail.kt must not participate in chrome hiding",
            navRailSrc.contains("AnimatedVisibility") || navRailSrc.contains("chromeVisible"),
        )
    }

    @Test
    fun `the width answer is provided once, above the whole tree`() {
        assertTrue(
            "MainActivity must provide LocalWideWindow from one isWideWindow() read",
            mainActivitySrc.contains("CompositionLocalProvider(LocalWideWindow provides isWideWindow())"),
        )
        assertFalse(
            "no screen may re-read the Configuration for width — LocalWideWindow is the answer",
            listOf(
                "NowScreen.kt", "TriageScreen.kt", "DoneScreen.kt", "LedgerScreen.kt",
                "AlertsScreen.kt", "NowLaneBoard.kt", "NavRail.kt",
            ).any { source(it).contains("screenWidthDp") },
        )
    }

    @Test
    fun `Now's wide branch renders the lane board as one entry of its one list`() {
        // The one hole a JVM suite would otherwise leave: delete the wide
        // branch and every other test stays green — only a device pass
        // notices the unfolded board reverting to a stretched single stack.
        // The key is load-bearing too: the dirty-Back handler scrolls to it
        // when a column-ranked pane is open on a wide window.
        val nowScreen = source("NowScreen.kt")
        val flat = nowScreen.replace(Regex("""\s+"""), " ")
        assertTrue(
            "NowScreen's populated branch must emit the lane board as the WIDE_BOARD_KEY " +
                "entry on a wide window, and keep the phone loop as the other arm",
            flat.contains("if (wide) item(key = WIDE_BOARD_KEY) { FrontierLaneBoard("),
        )
        assertTrue(
            "and the phone branch must remain the else arm, byte-identical",
            flat.contains("} else for (column in currentBoard.columns) {"),
        )
    }

    @Test
    fun `the grid shape and the lane board stay off the single-column surfaces`() {
        // Status is a panes surface, not an item list, and the detail/
        // takeover/settings screens are forms — the operator scoped
        // multi-column to the five list screens, and a grid creeping onto
        // Status would be scope drift with no decision behind it.
        for (name in listOf("StatusScreen.kt", "SettingsScreen.kt", "RulesScreen.kt", "RoutesScreen.kt")) {
            assertFalse(
                "$name must stay single-column (operator decision, the unfolded slice)",
                source(name).contains("LazyVerticalGrid") || source(name).contains("FrontierLaneBoard"),
            )
        }
    }
}
