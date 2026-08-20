package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The M4/#542 counterpart of `RulesScreenStructuralTest` (the file under
// test is `RecallOverlay.kt` since the search-overlay slice): `Core::search`
// (via `MobileTaskHost.search`) does every bit of Recall's matching,
// grouping and ordering core-side (ADR-0025) — this gate refuses a Kotlin
// copy of any of it. A hand-rolled `sortedBy` or `groupBy` here would
// compile, run, and look right on every fixture anyone would think to
// write; only a source gate catches it, which is why this test reads the
// files.
class RecallScreenStructuralTest {

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }

    /** The file's *code*, with comments removed — `RulesScreenStructuralTest`'s
     * own reasoning: a doc comment must be free to name the thing it
     * forbids without tripping the gate that forbids it. */
    private fun source(name: String) =
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/$name")
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")

    private val screenSrc by lazy { source("RecallOverlay.kt") }
    private val viewModelSrc by lazy { source("RecallViewModel.kt") }

    private val both by lazy {
        listOf(
            "RecallOverlay.kt" to screenSrc,
            "RecallViewModel.kt" to viewModelSrc,
        )
    }

    @Test
    fun `no recall surface sorts, groups or ranks a result set`() {
        // `Core::search` returns rows already matched, grouped
        // (live/done/archived) and ordered (most recently touched first
        // within a group); `total` is its own un-capped count. Neither
        // file may re-derive any of that.
        for ((name, src) in both) {
            for (spelling in listOf(
                "sortedBy", "sortWith", "sortedWith", ".sorted(",
                "groupBy", "compareBy", "Comparator",
            )) {
                assertFalse(
                    "$name must not order or group a result set — the seam already did ($spelling)",
                    src.contains(spelling),
                )
            }
        }
    }

    @Test
    fun `no recall surface filters or matches a query itself`() {
        // Matching (the multi-word AND, the handle lookup) is
        // `hummingbird_core::search::search`'s alone. A `.filter`/`.contains`
        // scan here over titles or descriptions would be a second, silently
        // drifting matcher.
        for ((name, src) in both) {
            assertFalse(
                "$name must not filter a row list itself",
                Regex("""rows\s*\.\s*filter""").containsMatchIn(src),
            )
            assertFalse(
                "$name must not test a title/description against the query itself",
                Regex("""\.(title|description)\s*\.\s*contains""").containsMatchIn(src),
            )
        }
    }

    @Test
    fun `total is read from the seam, never re-derived from the row list`() {
        assertTrue(
            "RecallOverlay must read the seam's own total",
            screenSrc.contains("total"),
        )
        for ((name, src) in both) {
            assertFalse(
                "$name must not stand in rows.size for the seam's total",
                Regex("""total\s*=\s*rows\.size""").containsMatchIn(src),
            )
        }
    }

    @Test
    fun `every when over MobileRecallGroup is exhaustive with no wildcard arm`() {
        val arm = Regex("""MobileRecallGroup\.[A-Z_]+\s*->""")
        assertTrue(
            "RecallOverlay.kt must map MobileRecallGroup by its variants",
            arm.containsMatchIn(screenSrc),
        )
        assertFalse(
            "RecallOverlay.kt must not fall back to a wildcard arm over MobileRecallGroup",
            Regex("""when\s*\([^)]*group[^)]*\)\s*\{[^}]*else\s*->""", RegexOption.DOT_MATCHES_ALL)
                .containsMatchIn(screenSrc),
        )
    }

    @Test
    fun `an inert row is dimmed - Done and archived share the alpha the live row does not`() {
        // #597: dimming tracks inertness, not the archive flag. A Done row
        // at full opacity carries every affordance cue of the live row
        // above it and answers none of them — the alpha is what tells the
        // truth. All three variants must be named (the wildcard test above
        // holds the no-else rule); this pins which side of the alpha each
        // lands on.
        assertTrue(
            "the live row must render solid",
            Regex("""MobileRecallGroup\.LIVE\s*->\s*1f""").containsMatchIn(screenSrc),
        )
        assertTrue(
            "a Done row must take the inert alpha (#597)",
            Regex("""MobileRecallGroup\.DONE\s*->\s*INERT_ALPHA""").containsMatchIn(screenSrc),
        )
        assertTrue(
            "an archived row must take the inert alpha",
            Regex("""MobileRecallGroup\.ARCHIVED\s*->\s*INERT_ALPHA""").containsMatchIn(screenSrc),
        )
    }

    @Test
    fun `only a live row expands, in place, into the shared panel`() {
        // The search-overlay slice: a live row's tap toggles the ViewModel's
        // selection and the shared ItemDetailPanel renders directly below
        // the row — the web RecallOverlay's own in-place expansion. Inert
        // rows (Done/archived) take no click at all, and nothing navigates:
        // Routes.ITEM_DETAIL stays the notification door's.
        assertTrue(
            "RecallOverlay.kt must gate the row tap on the LIVE group",
            screenSrc.contains("MobileRecallGroup.LIVE"),
        )
        assertTrue(
            "a live row's tap must drive the ViewModel's selection",
            screenSrc.contains("viewModel.select(row.id)"),
        )
        assertTrue(
            "the expanded row must render the shared ItemDetailPanel in place",
            screenSrc.contains("ItemDetailPanel("),
        )
        assertFalse(
            "the overlay must not navigate to item detail",
            screenSrc.contains("Routes.itemDetail"),
        )
    }

    @Test
    fun `the search door reaches MobileTaskHost search, not a second read`() {
        assertTrue(
            "RecallViewModel.kt must reach MobileTaskHost.search",
            viewModelSrc.contains(".search("),
        )
    }

    @Test
    fun `the overlay is AppRoot's, drawn over the NavHost, and Back closes it`() {
        // Not a route: AppRoot hosts the overlay as a sibling drawn after
        // the chrome Scaffold inside one Box, so it covers whatever screen
        // is showing — the web RecallOverlay's scrim-dialog contract. Back
        // closes the overlay itself (the deeper panel handlers still win
        // while an edit is dirty), never navigates the screen underneath.
        val main = source("MainActivity.kt")
        assertTrue(
            "AppRoot must draw RecallOverlay when the flag is up",
            main.contains("if (recallOpen) {"),
        )
        val box = main.substringAfter("Box {")
        assertTrue(
            "the overlay must be drawn after the Scaffold, inside the same Box",
            box.contains("Scaffold(") && box.indexOf("RecallOverlay(") > box.indexOf("Scaffold("),
        )
        assertTrue(
            "the overlay must own a BackHandler that closes it",
            Regex("""BackHandler\s*\{[\s\S]*?onClose\(\)""").containsMatchIn(screenSrc),
        )
        assertTrue(
            "the overlay's selection lives in the ViewModel, never a remember {}",
            screenSrc.contains("viewModel.selectedId") || screenSrc.contains("viewModel.clearSelection()"),
        )
    }

    @Test
    fun `a fresh open clears the expansion at the gesture, never in the composition`() {
        // A1a. `LaunchedEffect(Unit)` reads as "on open" and is not: it
        // re-fires on every Activity recreation, which on the install
        // target is every fold/unfold — collapsing a panel that was open,
        // and possibly mid-edit, a moment before. The reset belongs at the
        // state write that opens the surface.
        val main = source("MainActivity.kt")
        assertTrue(
            "AppRoot must open the overlay through one door that resets the selection",
            Regex("""fun openRecall\(\)\s*\{\s*recallViewModel\.clearSelection\(\)\s*recallOpen = true""")
                .containsMatchIn(main),
        )
        assertFalse(
            "no open gesture may raise the flag without going through that door",
            Regex("""recallOpen = true""").findAll(main).count() > 1,
        )
        assertFalse(
            "the overlay must not clear the selection from its own composition",
            Regex("""LaunchedEffect\(Unit\)\s*\{\s*viewModel\.clearSelection\(\)""")
                .containsMatchIn(screenSrc),
        )
        assertTrue(
            "the overlay must read AppRoot's own instance, not resolve a second one",
            main.contains("RecallOverlay(") &&
                Regex("""viewModel\s*=\s*recallViewModel""").containsMatchIn(main) &&
                Regex("""viewModel:\s*RecallViewModel,""").containsMatchIn(screenSrc),
        )
    }

    @Test
    fun `Back with a dirty draft scrolled out of view scrolls it back rather than dismissing`() {
        // A1b. The panel's own BackHandler is registered inside a
        // LazyColumn item; scrolling that item out of the viewport disposes
        // it, and the overlay's own Back then dismisses straight past the
        // discard confirmation, taking the words with it. `NowScreen`'s
        // guard, ported: the same keyed ViewModel lookup, the same
        // re-check, the same scroll-back-into-view answer.
        assertTrue(
            "the overlay must look the panel's ViewModel up by the panel's own key",
            screenSrc.contains("key = \"item-\$id\""),
        )
        assertTrue(
            "Back must re-check the draft's dirtiness for itself",
            screenSrc.contains("panelViewModel.isDirty"),
        )
        assertTrue(
            "and answer a dirty draft by scrolling it back into view",
            screenSrc.contains("listState.animateScrollToItem("),
        )
        assertTrue(
            "which needs the overlay's LazyColumn to own that state",
            screenSrc.contains("rememberLazyListState()") && screenSrc.contains("state = listState"),
        )
    }
}
