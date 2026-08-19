package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The bottom nav's contract (#532): a bar of four plus a More sheet, both
// generated from one Kotlin route list (`MainActivity.kt`'s
// `NavDestination`), pinned against the web's own `nav-bar.ts` — the same
// "repo-file helper reads web source" idiom `RulesScreenStructuralTest` and
// friends already use, pointed at `client/web/src` instead of
// `client/android` for the first time.
class BottomNavStructuralTest {

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }

    private val mainActivitySrc: String by lazy {
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/MainActivity.kt")
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
    }

    private val navBarTs: String by lazy {
        repoFile("client/web/src/shell/nav-bar.ts")
    }

    /** The web's `ON_THE_BAR` set, parsed straight out of `nav-bar.ts` — the
     * source of truth for "which four", not a hand-copied literal here that
     * could silently drift from it. */
    private val webBarScreens: Set<String> by lazy {
        val match = Regex("""ON_THE_BAR:\s*ReadonlySet<Screen>\s*=\s*new Set<Screen>\(\[([^\]]*)\]\)""")
            .find(navBarTs)
            ?: error("could not find nav-bar.ts's ON_THE_BAR set literal")
        Regex(""""([a-z]+)"""").findAll(match.groupValues[1]).map { it.groupValues[1] }.toSet()
    }

    /** `MainActivity.kt`'s `NavDestination` enum, parsed into
     * (route-constant, onBar) pairs, then resolved against `Routes`' own
     * `const val` declarations — the same two-parse-then-join `Navigation
     * StructuralTest` doesn't need but this test does, since the enum
     * refers to `Routes.NOW` rather than the literal `"now"`. */
    private data class NavEntry(val routeConst: String, val onBar: Boolean)

    private val routesConsts: Map<String, String> by lazy {
        val block = mainActivitySrc.substringAfter("private object Routes {").substringBefore("\n}")
        Regex("""const val (\w+) = "([a-z_{}/]+)"""")
            .findAll(block)
            .associate { it.groupValues[1] to it.groupValues[2] }
    }

    private val navDestinations: List<NavEntry> by lazy {
        val block = mainActivitySrc
            .substringAfter("private enum class NavDestination(")
            .substringBefore("\n}")
        Regex("""(\w+)\(Routes\.(\w+),\s*"[^"]*",\s*onBar\s*=\s*(true|false)\)""")
            .findAll(block)
            .map { NavEntry(routeConst = it.groupValues[2], onBar = it.groupValues[3] == "true") }
            .toList()
    }

    private fun route(entry: NavEntry): String =
        routesConsts[entry.routeConst] ?: error("Routes.${entry.routeConst} is not a const val")

    /** `Routes`' top-level screens: every `const val` whose value carries no
     * `{…}` placeholder — a detail/takeover route (`ALERT_DETAIL`,
     * `ITEM_DETAIL`, `GRILL`) is parameterised and reached by `navigate(...)`
     * from inside a screen, never from the bar or the sheet, so it is not
     * part of this nav form's universe at all. `RECALL` is excluded for a
     * different reason: it is a gesture entry point, not a screen — the same
     * distinction the web's `nav-bar.ts` draws by keeping its `onSearch` row
     * outside `NAV_BAR_OVERFLOW` and `SCREENS` entirely, so it carries no
     * `NavDestination` entry here either and this universe stays "the nine
     * screens", matching the web's own `SCREENS` count. */
    private val topLevelScreenConsts: Set<String> by lazy {
        routesConsts.filterValues { !it.contains("{") }.keys - "RECALL"
    }

    /** The nav form's exception mechanism: a place to name a top-level
     * screen `Routes` declares whose reachability is deliberately left to a
     * later slice, rather than landing in `NavDestination`. Named and
     * explicit so such a screen fails loudly instead of passing by
     * construction. **Empty now** — #541 (this issue) wired Rules' and
     * Settings' reachability and closed out the last entries it held, and
     * nothing is presently deferred. The mechanism stays for the next
     * screen that needs it. */
    private val deferredToLaterSlice = emptySet<String>()

    @Test
    fun `the bar carries exactly the web's four acting surfaces`() {
        val androidBar = navDestinations.filter { it.onBar }.map { route(it) }.toSet()
        assertEquals(webBarScreens, androidBar)
        assertEquals(4, androidBar.size)
    }

    @Test
    fun `a bar destination renders no back affordance`() {
        // #588 item 1: a bottom-bar tab has nothing to go back to — its
        // stack root is itself. The Back links the bar tabs carried were
        // web chrome ported from a surface that had no bar; the More-sheet
        // screens keep theirs (they are pushed and poppable). Comments are
        // stripped so a doc sentence may name what this forbids.
        for (entry in navDestinations.filter { it.onBar }) {
            val screenFile = route(entry).replaceFirstChar { it.uppercase() } + "Screen.kt"
            val src = repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/$screenFile")
                .replace(Regex("""/\*[\s\S]*?\*/"""), "")
                .replace(Regex("""(?m)^\s*//.*$"""), "")
            assertFalse(
                "$screenFile is a bar tab and must not take an onBack callback (#588)",
                // Word-bounded so onBackground/onBackPressed-style names
                // stay legal; the callback name itself is the ban.
                Regex("""\bonBack\b""").containsMatchIn(src),
            )
            assertFalse(
                "$screenFile is a bar tab and must not render a Back control (#588)",
                // The defect's exact string shapes ("Back", "Back to Now"),
                // not any string starting with Back — "Background sync
                // paused" must not trip a nav gate.
                Regex(""""Back( to [A-Za-z]+)?"""").containsMatchIn(src),
            )
        }
    }

    @Test
    fun `bar and sheet cover every top-level screen Routes declares, with no pending exceptions`() {
        // The AC's real claim: nothing may go missing from `Routes`
        // silently. A tenth screen added there now fails this test until it
        // lands on the bar, in the sheet, or is added to
        // `deferredToLaterSlice` with a name and a reason — which is itself
        // a change someone has to make and justify, not a side effect of
        // `NavDestination`'s own filter/filterNot partition.
        val navDestinationConsts = navDestinations.map { it.routeConst }.toSet()
        assertEquals(
            "every Routes top-level screen must be on the bar, in the sheet, or named in " +
                "deferredToLaterSlice — dropping off all three is the silent failure this " +
                "test exists to catch",
            topLevelScreenConsts,
            navDestinationConsts + deferredToLaterSlice,
        )
        assertTrue(
            "deferredToLaterSlice is empty since #541 — all nine screens are reachable now, " +
                "so any entry here would be a satisfied exception left behind",
            deferredToLaterSlice.isEmpty(),
        )
    }

    @Test
    fun `no route sits on both the bar and in the sheet`() {
        val bar = navDestinations.filter { it.onBar }.map { route(it) }
        val overflow = navDestinations.filterNot { it.onBar }.map { route(it) }
        assertTrue(bar.intersect(overflow.toSet()).isEmpty())
        assertEquals((bar + overflow).size, (bar + overflow).toSet().size)
    }

    @Test
    fun `every NavDestination route has a registered composable, and vice versa for the top-level screens`() {
        for (entry in navDestinations) {
            val routeString = route(entry)
            assertTrue(
                "MainActivity must register composable(Routes.${entry.routeConst})",
                mainActivitySrc.contains("composable(Routes.${entry.routeConst})") ||
                    mainActivitySrc.contains("composable(Routes.${entry.routeConst}) {"),
            )
            assertTrue(
                "route string \"$routeString\" is missing from Routes",
                routesConsts.containsValue(routeString),
            )
        }
        // The reverse direction the test's own name promises: every
        // registered composable() for a top-level screen must be reachable
        // from `NavDestination` (bar, sheet) or be named in the exception
        // list — the same guard as the coverage test above, restated as
        // "every composable, not just every route string".
        for (routeConst in topLevelScreenConsts) {
            val reachable = routeConst in navDestinations.map { it.routeConst } ||
                routeConst in deferredToLaterSlice
            assertTrue(
                "Routes.$routeConst has a composable() but is reachable from neither the bar, " +
                    "the sheet, nor deferredToLaterSlice",
                reachable,
            )
        }
    }

    @Test
    fun `Done, the Ledger, Rules, Settings and Routes sit in the sheet, not on the bar`() {
        val overflow = navDestinations.filterNot { it.onBar }.map { it.routeConst }.toSet()
        assertEquals(setOf("DONE", "LEDGER", "RULES", "SETTINGS", "ROUTES"), overflow)
    }

    @Test
    fun `the More control reads as current whenever the open screen is in the sheet`() {
        // The bar's own correction (`nav-bar.ts`'s `isOverflowScreen`,
        // ported): without it, the bar shows nothing selected while an
        // overflow screen is open, which reads as "you are nowhere".
        val body = mainActivitySrc.substringAfter("private fun BottomNavBar(").substringBefore("\n}\n")
        assertTrue(
            "BottomNavBar must compute an overflow-active flag for the More control",
            body.contains("overflowActive"),
        )
        assertTrue(
            "the More control's selected state must read the overflow-active flag",
            body.contains("selected = overflowActive"),
        )
    }

    @Test
    fun `the More sheet carries a Recall entry point, reachable but not a top-level screen`() {
        // #541's other half of the AC: "all nine screens ... plus the
        // Recall entry." Routes.RECALL is deliberately absent from
        // `NavDestination` (see `topLevelScreenConsts`'s own doc), so this
        // is the one place that pins its reachability instead.
        assertTrue(
            "MainActivity must register composable(Routes.RECALL)",
            mainActivitySrc.contains("composable(Routes.RECALL)"),
        )
        val sheetBody = mainActivitySrc.substringAfter("private fun MoreSheet(").substringBefore("\n}\n")
        assertTrue(
            "the More sheet must offer a way to Routes.RECALL",
            sheetBody.contains("Routes.RECALL"),
        )
        assertTrue(
            "RECALL must not be one of NavDestination's entries — it is a gesture, not a screen",
            navDestinations.none { it.routeConst == "RECALL" },
        )
    }
}
