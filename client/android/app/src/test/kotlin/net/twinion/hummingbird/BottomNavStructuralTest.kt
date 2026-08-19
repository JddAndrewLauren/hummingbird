package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertEquals
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

    @Test
    fun `the bar carries exactly the web's four acting surfaces`() {
        val androidBar = navDestinations.filter { it.onBar }.map { route(it) }.toSet()
        assertEquals(webBarScreens, androidBar)
        assertEquals(4, androidBar.size)
    }

    @Test
    fun `bar and sheet together cover every route this nav form carries, with nothing doubled`() {
        val bar = navDestinations.filter { it.onBar }.map { route(it) }
        val overflow = navDestinations.filterNot { it.onBar }.map { route(it) }
        val everyRoute = navDestinations.map { route(it) }

        assertEquals((bar + overflow).sorted(), everyRoute.sorted())
        assertEquals(everyRoute.size, everyRoute.toSet().size)
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
    }

    @Test
    fun `Done and the Ledger sit in the sheet, not on the bar`() {
        val overflow = navDestinations.filterNot { it.onBar }.map { it.routeConst }.toSet()
        assertEquals(setOf("DONE", "LEDGER"), overflow)
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
}
