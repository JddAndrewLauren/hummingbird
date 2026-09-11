package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The M3/#531 counterpart of `RulesScreenStructuralTest`: the gates this
// slice's own acceptance criteria most need, since nothing else catches a
// re-derived count or a smuggled-in Grill interview at review time.
//
// Since the panel unification this file gates what remains *this screen's*:
// the counts, the queue rows, the selection expanding in place (#659), and
// the Back guard that lives above the grid. Everything the opened pane shows and does is
// `ItemDetailPanelStructuralTest`'s — including the header, the field set
// and the mark-done check, which used to be pinned here against a second
// editor implementation.
class TriageScreenStructuralTest {

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see client/android/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }

    private fun source(name: String) =
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/$name")
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")

    private val screenSrc by lazy { source("TriageScreen.kt") }
    private val viewModelSrc by lazy { source("TriageViewModel.kt") }
    private val both by lazy {
        listOf("TriageScreen.kt" to screenSrc, "TriageViewModel.kt" to viewModelSrc)
    }

    @Test
    fun `the header counts are read from the record, never recomputed`() {
        assertTrue(
            "the header must read the board's own capturedCount",
            screenSrc.contains("board.capturedCount"),
        )
        assertTrue(
            "the header must read the board's own grillingCount",
            screenSrc.contains("board.grillingCount"),
        )
        for ((name, src) in both) {
            assertFalse(
                "$name must not recompute a count from items.size/items.count",
                Regex("""items\.(size|count)""").containsMatchIn(src),
            )
        }
    }

    @Test
    fun `the Grill button is live and navigates to the takeover, never holds its own interview state`() {
        assertTrue(
            "TriageScreen.kt must render a Grill button",
            screenSrc.contains("onGrill"),
        )
        // The button is gated on the row's own can-grill fact from the
        // seam, never a hand-rolled stage check — and it navigates rather
        // than opening an interview inline. #539 lands the takeover as a
        // separate screen/ViewModel (`GrillTakeoverScreen.kt`,
        // `GrillTakeoverViewModel.kt`); nothing here may hold a turn, a
        // session, or a grill draft state.
        for ((name, src) in both) {
            for (spelling in listOf("GrillTurn", "grillDraft", "GrillSession", "grillTurn")) {
                assertFalse(
                    "$name must not carry any Grill interview state ($spelling)",
                    src.contains(spelling),
                )
            }
        }
    }

    /** #360 after the panel unification: the pane Triage opens is
     * `ItemDetailPanel`, whose ViewModel also carries a plain `save`. The
     * *mode* is what keeps that unreachable from here, so the mode is what
     * this file pins; the literal `triageItem(itemId, true,` behind it is
     * pinned by `ItemDetailPanelStructuralTest`, at the factory that writes
     * it. */
    @Test
    fun `promoting to Ready is the only save destination offered`() {
        assertTrue(
            "the opened pane must be rendered in the promoting mode",
            screenSrc.contains("mode = ItemDetailPanelMode.PROMOTE"),
        )
        for ((name, src) in both) {
            assertFalse(
                "$name must not render the panel's saving mode",
                src.contains("ItemDetailPanelMode.SAVE"),
            )
            assertFalse(
                "$name must not reach the panel ViewModel's non-promoting save",
                src.contains(".save("),
            )
            assertFalse(
                "$name must not offer a non-promoting save of its own",
                src.contains("saveEdits"),
            )
        }
    }

    @Test
    fun `mark-done goes through act, never a second triage`() {
        assertTrue(
            "TriageViewModel.kt must call the existing act path via completeFn",
            viewModelSrc.contains("\"complete\""),
        )
    }

    @Test
    fun `only one row may be open at a time`() {
        assertTrue(
            "TriageViewModel.kt must track exactly one selected row",
            viewModelSrc.contains("_selectedId"),
        )
    }

    /** The route is unreachable until #532, so nothing exercises this on a
     * real device pass yet — the review finding on this PR that made this
     * gate necessary. `AlertsScreen`'s own precedent: a foreground resume
     * must re-read independent of the app-wide `syncTick`, or a capture
     * minted elsewhere while this screen was backgrounded waits for the
     * next tick to appear. */
    @Test
    fun `the screen re-reads on every foreground resume, not just on syncTick`() {
        assertTrue(
            "TriageScreen.kt must carry a LifecycleResumeEffect",
            screenSrc.contains("LifecycleResumeEffect"),
        )
        assertTrue(
            "the resume effect must reload, cancelling on pause/dispose",
            screenSrc.contains("onPauseOrDispose"),
        )
    }

    @Test
    fun `the triage route is registered and reachable from the bottom nav bar`() {
        val main = source("MainActivity.kt")
        assertTrue(
            "MainActivity must register the triage route",
            main.contains("composable(Routes.TRIAGE)"),
        )
        // #532 gave it a home: `NavDestination.TRIAGE` with `onBar = true`
        // is the one route list generating the bottom bar, so Triage is
        // reachable through it — never through a second, ad-hoc
        // `navigate(Routes.TRIAGE)` call, which would be a hand-rolled
        // door for a screen the bar already carries.
        assertTrue(
            "TRIAGE must be a NavDestination entry with onBar = true",
            Regex("""TRIAGE\(Routes\.TRIAGE,\s*"[^"]*",\s*onBar\s*=\s*true\)""").containsMatchIn(main),
        )
        assertFalse(
            "no ad-hoc navigate(Routes.TRIAGE) — the bottom nav's goToTab is the one door",
            main.contains("navigate(Routes.TRIAGE)"),
        )
    }

    @Test
    fun `the rows render through the shared NowRow, never a second card implementation`() {
        // The Triage-parity slice: the queue's collapsed rows are the SAME
        // compact card the Now screen renders (`NowRow.kt`), fed by the
        // adapter that copies the record's decided fields verbatim — so the
        // calm-gets-nothing/ready-says-nothing/judged-only-glyph rules can
        // never fork per surface, and the urgency band arrives decided from
        // the seam rather than being recomputed here.
        assertTrue(
            "the queue rows must render through the shared NowRow",
            screenSrc.contains("NowRow("),
        )
        assertTrue(
            "the record crosses through its verbatim adapter",
            screenSrc.contains(".asRowModel()"),
        )
        for ((name, src) in both) {
            assertFalse(
                "$name must not re-derive an urgency band (compute/deadline arithmetic is the seam's)",
                src.contains("computeUrgency") || src.contains("urgencyColor("),
            )
        }
    }

    @Test
    fun `the opened capture expands in place, in the tapped row's own slot, the Now pattern`() {
        // #659, NowScreen's 2026-08-20 shape: the pane is an item INSIDE the
        // queue's one scrollable — a LazyVerticalGrid since the unfolded
        // slice, one fixed column on the phone — and WHERE inside is the
        // point: the selected record renders as the pane in its own slot
        // and every other record as its row, so the card the operator
        // tapped grows rather than a block appearing at the top of the
        // queue. The pane IS `ItemDetailPanel`, with #360 kept by the mode
        // above instead of by a second implementation. Comments are
        // stripped, so these are code positions.
        val flat = screenSrc.replace(Regex("""\s+"""), " ")
        val lazyColumn = screenSrc.indexOf("LazyVerticalGrid(")
        val queueBranch = screenSrc.indexOf("board != null -> for (item in board.items)")
        val paneEmit = screenSrc.indexOf("item(", queueBranch)
        assertTrue("TriageScreen must keep one LazyVerticalGrid", lazyColumn >= 0)
        assertTrue("the queue loop must sit inside the grid", queueBranch > lazyColumn)
        assertFalse(
            "the pane must not be an entry of its own before the queue's branches — " +
                "that was the index-0 block",
            screenSrc.substring(lazyColumn, queueBranch).contains("selected-item"),
        )

        // The row-or-pane branch, bounded to the queue loop: every item
        // draws exactly one of the two, so the queue keeps one line per
        // item and no item is drawn twice (the pane's header is the title,
        // its action row the mark-done check).
        val queueLoop = flat.substring(flat.indexOf("board != null -> for (item in board.items)"))
        assertTrue(
            "the selected record must render as the pane in its own slot, spanning every " +
                "grid lane — full width whatever the column count",
            queueLoop.contains(
                "if (item.id == selectedId) { item( key = selectedItemKey(item.id), " +
                    "span = { GridItemSpan(maxLineSpan) }, ) { Card(",
            ),
        )
        assertTrue(
            "and every other record as its row, never as a selected one",
            queueLoop.contains("} else { item(key = item.id) { NowRow( record = item.asRowModel(), dark = dark, selected = false,"),
        )
        assertTrue(
            "the expanded pane must be the shared ItemDetailPanel",
            paneEmit >= 0 && screenSrc.indexOf("ItemDetailPanel(", paneEmit) > paneEmit,
        )

        // The slot key **names the item**, and a constant one is a defect,
        // not a style: it makes the panel's disposal-and-recompose land on
        // the same `SaveableStateHolder` slot, which handed item B the state
        // item A saved there (`README`'s "The title-edit trap").
        assertTrue(
            "the pane's key must name the item",
            screenSrc.contains("private fun selectedItemKey(itemId: String) = \"selected-item-\$itemId\""),
        )
        assertFalse(
            "and the key must not go back to a constant — that is the leak",
            screenSrc.contains("\"selected-item\""),
        )

        // **Nothing scrolls on a selection.** The pane opens where the
        // finger already is; the jump to 0 was what made the first tap and
        // the second look like different gestures. No effect keyed on the
        // selection may touch the grid state, and no scroll may target a
        // literal index.
        assertFalse(
            "no selection-change effect may scroll the grid",
            screenSrc.contains("LaunchedEffect(selectedId)") || screenSrc.contains("lastScrolledSelection"),
        )
        assertFalse(
            "and nothing may scroll to a literal index — the pane is never at 0 by construction",
            Regex("""(animateScrollToItem|scrollToItem)\(\d""").containsMatchIn(screenSrc),
        )
        assertFalse(
            "the retired second editor must not come back",
            screenSrc.contains("TriageEditorPanel"),
        )
        // The whole point of the unification: no field widget of this
        // screen's own. Every editor the pane shows is the panel's, which
        // is `ui/forms`'.
        assertFalse(
            "TriageScreen must hold no text field of its own",
            screenSrc.contains("OutlinedTextField("),
        )
    }

    @Test
    fun `Back with a dirty draft is guarded, by the app's one discard dialog, from the screen`() {
        // A4: human-authored content is never silently thrown away
        // (`ItemDetailPanel`'s header states the house rule).
        //
        // Two placements are load-bearing. The handler is registered at the
        // SCREEN, not inside the pane's LazyColumn item: an item scrolled
        // out of the viewport is disposed, taking its handler with it (the
        // defect `NowScreen`'s own guard exists for). And the dirtiness it
        // reads comes from the panel's own ViewModel, resolved by the
        // panel's own key — a lookup under any other key is a DIFFERENT
        // instance, which would report a clean draft while the pane holds a
        // dirty one.
        // Whitespace-collapsed, and the WHOLE guard rather than any line
        // of it: `panelViewModel?.isDirty` appears twice in this file (the
        // re-tap guard below is the other), so an assertion that only
        // looked for that spelling stayed green with this handler gutted —
        // green for the wrong one of two indistinguishable reasons.
        //
        // Since #659 the pane is the selected row's own slot, so the scroll
        // target is no longer 0 for free: the handler re-reads the pane's
        // index from the live layout by the pane's key, falling back to the
        // position it was last seen at, and — NowScreen's fallthrough — only
        // while the board still carries the item. When the pane is not in
        // the grid at all it CLOSES rather than scrolls; otherwise Back is
        // dead while a dirty draft can never be re-seeded (no dialog, no
        // close, no way out).
        val flat = screenSrc.replace(Regex("""\s+"""), " ")
        assertTrue(
            "the screen must guard Back whenever a pane is open, and route a dirty " +
                "draft to the panel's own dialog rather than closing the pane — " +
                "scrolling to the pane's OWN index, gated on board membership",
            flat.contains(
                "BackHandler(enabled = selectedId != null) { " +
                    "val paneIndex = selectedId ?.takeIf { id -> board?.items?.any { it.id == id } == true } " +
                    "?.let { id -> val key = selectedItemKey(id) " +
                    "listState.layoutInfo.visibleItemsInfo.firstOrNull { it.key == key }?.index " +
                    "?: lastSeenPanePosition } " +
                    "if (paneIndex != null && panelViewModel?.isDirty == true) { " +
                    "scope.launch { listState.animateScrollToItem(paneIndex) } " +
                    "} else { viewModel.closeSelection() } }",
            ),
        )
        assertTrue(
            "the fallback position must be captured from the layout while the pane is on " +
                "screen, keyed on the selection so a stale index never names another row",
            flat.contains(
                "var lastSeenPanePosition by remember(selectedId) { mutableStateOf<Int?>(null) } " +
                    "LaunchedEffect(listState, selectedId) { " +
                    "val key = selectedId?.let { selectedItemKey(it) } ?: return@LaunchedEffect " +
                    "snapshotFlow { listState.layoutInfo.visibleItemsInfo.firstOrNull { it.key == key }?.index " +
                    "}.collect { index -> if (index != null) lastSeenPanePosition = index } }",
            ),
        )
        val handler = screenSrc.indexOf("BackHandler(")
        val lazyColumn = screenSrc.indexOf("LazyVerticalGrid(")
        assertTrue("the guard must be registered above the grid, never inside an item", handler in 0 until lazyColumn)
        assertTrue(
            "the guard must ask the panel's own ViewModel, under the panel's own key",
            screenSrc.contains(
                "ItemDetailViewModel.factory(context), key = \"item-" + "\$" + "id\"",
            ),
        )
        // The confirmation is the panel's — this screen holds no dialog of
        // its own, and the house is dialog-wary (`ItemDetailPanel`'s header
        // claims to hold the only one).
        assertFalse(
            "no dialog on this screen: the panel owns the one discard confirmation",
            screenSrc.contains("AlertDialog(") || screenSrc.contains("DiscardConfirmation("),
        )
    }

    /** Every leaving gesture asks the same question. Re-tapping the open
     * row used to be `select(sameId)`, a toggle shut — and the one exit
     * that did not pass through the panel, so the screen had to guard it
     * itself. Since #659 the open item is never drawn as a row, so that
     * gesture is unreachable from here and the guard with it: the pane's
     * own header row is a close target sitting where the row was, and it
     * routes through the panel's `DiscardConfirmation` like the X and Back
     * inside the pane. So the rows carry the plain select and no dirtiness
     * check of their own — a second copy of the guard here would be dead
     * code that reads as a live one. */
    @Test
    fun `the rows carry no re-tap guard of their own — the pane's header is the close target`() {
        assertTrue(
            "a row tap is the plain select — a tap on a different row keeps replace semantics",
            screenSrc.contains("onOpen = { viewModel.select(item.id) }"),
        )
        assertFalse(
            "no row may re-check dirtiness — the open item has no row to re-tap",
            screenSrc.contains("item.id == selectedId && panelViewModel?.isDirty"),
        )
        // The guard is asked exactly once on this screen: by Back.
        assertEquals(
            "panelViewModel?.isDirty is read by the Back guard alone",
            1,
            Regex("""panelViewModel\?\.isDirty""").findAll(screenSrc).count(),
        )
    }

    @Test
    fun `no triage surface writes its own blank check`() {
        for ((name, src) in both) {
            assertFalse(
                "$name must not use isBlank — canSubmitCapture is the rule",
                src.contains("isBlank("),
            )
            assertFalse(
                "$name must not use isNotBlank — canSubmitCapture is the rule",
                src.contains("isNotBlank("),
            )
            assertFalse(
                "$name must not trim on the caller's behalf",
                src.contains(".trim("),
            )
        }
    }
}
