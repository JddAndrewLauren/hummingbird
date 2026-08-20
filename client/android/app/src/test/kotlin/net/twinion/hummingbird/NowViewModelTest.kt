package net.twinion.hummingbird

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileFrontierAxis
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswer
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswerState
import uniffi.hummingbird_ffi_mobile.MobilePaneBand
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileStandingQuestion
import uniffi.hummingbird_ffi_mobile.MobileUrgencyBand
import uniffi.hummingbird_ffi_mobile.MobileZoneFact
import uniffi.hummingbird_ffi_mobile.MobileZoneQuery
import uniffi.hummingbird_ffi_mobile.NowBoardRecord
import uniffi.hummingbird_ffi_mobile.NowColumnRecord
import uniffi.hummingbird_ffi_mobile.NowFacetSelectionRecord
import uniffi.hummingbird_ffi_mobile.NowItemRecord

// NowViewModel's load/refresh/act/setAxis/toggleFacet/toggleCollapsed
// control flow, exercised entirely with fakes — the same "no generated JNI
// binding involved" reasoning CaptureViewModelTest states for its own
// class. The production wiring's own correctness — that [NowViewModel.create]
// really reaches `MobileTaskHost.nowBoard`/`.act`/`FrontierPrefs` and that
// nothing on this screen re-derives ordering, grouping, urgency or the act
// vocabulary locally — is NowScreenStructuralTest's job.
class NowViewModelTest {

    private fun record(id: String, actions: List<String> = listOf("start")) = NowItemRecord(
        id = id,
        title = "item $id",
        deadline = null,
        urgency = MobileUrgencyBand.CALM,
        priority = 0L,
        context = null,
        size = null,
        energy = null,
        availableActions = actions,
        stage = "ready",
        canMarkDone = true,
    )

    private fun board(vararg ids: String, liveColumnKeys: List<String> = listOf("")) = NowBoardRecord(
        columns = listOf(
            NowColumnRecord(value = null, label = null, items = ids.map { record(it) }),
        ),
        blocked = emptyList(),
        contexts = emptyList(),
        liveColumnKeys = liveColumnKeys,
        shownCount = ids.size.toUInt(),
        totalCount = ids.size.toUInt(),
    )

    private fun columnIds(board: NowBoardRecord): List<String> =
        board.columns.flatMap { column -> column.items.map { it.id } }

    private fun pane(question: MobileStandingQuestion, band: MobilePaneBand = MobilePaneBand.DORMANT) =
        MobileRankedPane(
            standingQuestion = question,
            subjectKey = "the-subject",
            paneKey = "${question.name.lowercase()}:the-subject",
            answer = MobilePaneAnswer(
                answerState = MobilePaneAnswerState.UNBOUND,
                band = band,
                withinBand = null,
            ),
        )

    private fun viewModel(
        fetchBoardFn: suspend (MobileFrontierAxis, NowFacetSelectionRecord, String) -> NowBoardRecord = { _, _, _ -> board() },
        readAxisFn: suspend () -> MobileFrontierAxis = { MobileFrontierAxis.CONTEXT },
        writeAxisFn: suspend (MobileFrontierAxis) -> Unit = {},
        readCollapsedFn: suspend () -> Set<String> = { emptySet() },
        writeCollapsedFn: suspend (Set<String>) -> Unit = {},
        paneZoneQueriesFn: suspend (Long) -> List<MobileZoneQuery> = { emptyList() },
        rankPanesFn: suspend (Long, List<MobileZoneFact>) -> List<MobileRankedPane> = { _, _ -> emptyList() },
        setScheduledDateFn: suspend (String, String?, Long) -> Unit = { _, _, _ -> },
        completeFn: suspend (String, Long) -> Unit = { _, _ -> },
    ) = NowViewModel(
        fetchBoardFn,
        readAxisFn,
        writeAxisFn,
        readCollapsedFn,
        writeCollapsedFn,
        paneZoneQueriesFn,
        rankPanesFn,
        setScheduledDateFn,
        completeFn,
    )

    @Test
    fun `load restores the persisted axis and collapse set before the first fetch`() = runBlocking {
        var seenAxis: MobileFrontierAxis? = null
        val vm = viewModel(
            fetchBoardFn = { axis, _, _ -> seenAxis = axis; board() },
            readAxisFn = { MobileFrontierAxis.SIZE },
            readCollapsedFn = { setOf("@phone") },
        )

        vm.load("2026-08-15T12:00")

        assertEquals(MobileFrontierAxis.SIZE, vm.axis.value)
        assertEquals(MobileFrontierAxis.SIZE, seenAxis)
        assertEquals(setOf("@phone"), vm.collapsed.value)
        assertFalse("loading must settle false once the fetch returns", vm.loading.value)
    }

    /** The row checkmark: [NowViewModel.complete] acts through the seam,
     * drops the selection when it pointed at the completed row (a finished
     * item's panel must not stay standing over a board that no longer
     * holds it), and re-reads the board in the same gesture. */
    @Test
    fun `complete acts, closes the completed row's own panel, and re-reads the board`() = runBlocking {
        var acted: Pair<String, Long>? = null
        var fetches = 0
        val vm = viewModel(
            fetchBoardFn = { _, _, _ -> fetches += 1; board("a") },
            completeFn = { itemId, nowMs -> acted = itemId to nowMs },
        )
        vm.load("2026-08-19T12:00")
        vm.selectItem("a")

        vm.complete("a", "2026-08-19T12:01", 42L)

        assertEquals("a" to 42L, acted)
        assertNull("the completed row's panel must close", vm.selectedItemId.value)
        assertEquals("load once, then the post-complete re-read", 2, fetches)
        assertNull(vm.statusLine.value)
    }

    /** A failed complete keeps the board honest — the re-read still runs,
     * an unrelated selection survives, and the failure is said in words
     * rather than swallowed. */
    @Test
    fun `a failed complete re-reads anyway, keeps another row's panel, and says so`() = runBlocking {
        val vm = viewModel(
            completeFn = { _, _ -> throw RuntimeException("boom") },
        )
        vm.load("2026-08-19T12:00")
        vm.selectItem("other")

        vm.complete("a", "2026-08-19T12:01", 42L)

        assertEquals("other", vm.selectedItemId.value)
        assertEquals("Couldn't complete — boom", vm.statusLine.value)
    }

    /** #530's "rendering it makes a single crossing": entry reads the board
     * once. `NowScreen` used to run a one-shot `LaunchedEffect { load }`
     * beside its resume effect's `refresh`, so opening Now crossed the seam
     * twice — and the two raced, letting a default-axis board win over the
     * persisted one. The resume path now branches on [NowViewModel
     * .loadedOnce], which this pins on both sides: unset until a `load`
     * completes, set afterwards. */
    @Test
    fun `loadedOnce is false until a load completes, so entry loads exactly once`() = runBlocking {
        var fetches = 0
        val vm = viewModel(fetchBoardFn = { _, _, _ -> fetches += 1; board() })

        assertFalse("a fresh ViewModel has not loaded", vm.loadedOnce)

        vm.load("2026-08-15T12:00")

        assertTrue("load must mark the instance loaded", vm.loadedOnce)
        assertEquals("entry must read the board exactly once", 1, fetches)
    }

    @Test
    fun `refresh loads whatever the injected fetch fn returns, in its own order`() = runBlocking {
        val vm = viewModel(fetchBoardFn = { _, _, _ -> board("b", "a") })

        vm.refresh("2026-08-15T12:00")

        assertEquals(listOf("b", "a"), columnIds(vm.board.value!!))
    }

    @Test
    fun `refresh passes the given deadline-shaped now straight through`() = runBlocking {
        var seenNow: String? = null
        val vm = viewModel(fetchBoardFn = { _, _, now -> seenNow = now; board() })

        vm.refresh("2026-08-15T09:30")

        assertEquals("2026-08-15T09:30", seenNow)
    }

    @Test
    fun `setAxis writes the new axis, clears collapse and expand, and reloads under it`() = runBlocking {
        var writtenAxis: MobileFrontierAxis? = null
        var writtenCollapsed: Set<String>? = null
        var seenAxis: MobileFrontierAxis? = null
        val vm = viewModel(
            fetchBoardFn = { axis, _, _ -> seenAxis = axis; board() },
            writeAxisFn = { axis -> writtenAxis = axis },
            writeCollapsedFn = { collapsed -> writtenCollapsed = collapsed },
        )
        vm.toggleCollapsed("@phone")
        vm.toggleExpanded("@phone")

        vm.setAxis(MobileFrontierAxis.PROJECT, "2026-08-15T12:00")

        assertEquals(MobileFrontierAxis.PROJECT, vm.axis.value)
        assertEquals(MobileFrontierAxis.PROJECT, writtenAxis)
        assertEquals(MobileFrontierAxis.PROJECT, seenAxis)
        assertEquals(emptySet<String>(), vm.collapsed.value)
        assertEquals(emptySet<String>(), writtenCollapsed)
        assertEquals(emptySet<String>(), vm.expanded.value)
    }

    @Test
    fun `toggleFacet adds then removes a value and reloads under the current selection each time`() = runBlocking {
        var seenFacets: NowFacetSelectionRecord? = null
        val vm = viewModel(fetchBoardFn = { _, facets, _ -> seenFacets = facets; board() })

        vm.toggleFacet(FrontierFacet.CONTEXT, "@phone", "2026-08-15T12:00")
        assertEquals(setOf("@phone"), vm.facets.value.context)
        assertEquals(listOf("@phone"), seenFacets?.context)

        vm.toggleFacet(FrontierFacet.CONTEXT, "@phone", "2026-08-15T12:00")
        assertEquals(emptySet<String>(), vm.facets.value.context)
        assertEquals(emptyList<String>(), seenFacets?.context)
    }

    @Test
    fun `clearFacets resets every facet and reloads`() = runBlocking {
        val vm = viewModel()
        vm.toggleFacet(FrontierFacet.CONTEXT, "@phone", "2026-08-15T12:00")
        vm.toggleFacet(FrontierFacet.SIZE, "quick", "2026-08-15T12:00")

        vm.clearFacets("2026-08-15T12:00")

        assertEquals(FrontierFacetSelection(), vm.facets.value)
    }

    @Test
    fun `facet selection is never handed to the axis-collapse persistence doors`() = runBlocking {
        var collapsedWrites = 0
        var axisWrites = 0
        val vm = viewModel(
            writeAxisFn = { axisWrites++ },
            writeCollapsedFn = { collapsedWrites++ },
        )

        vm.toggleFacet(FrontierFacet.CONTEXT, "@phone", "2026-08-15T12:00")
        vm.clearFacets("2026-08-15T12:00")

        assertEquals(0, axisWrites)
        assertEquals(0, collapsedWrites)
    }

    @Test
    fun `toggleCollapsed adds then removes a key and persists each write`() = runBlocking {
        val writes = mutableListOf<Set<String>>()
        val vm = viewModel(writeCollapsedFn = { writes.add(it) })

        vm.toggleCollapsed("@phone")
        assertEquals(setOf("@phone"), vm.collapsed.value)

        vm.toggleCollapsed("@phone")
        assertEquals(emptySet<String>(), vm.collapsed.value)

        assertEquals(listOf(setOf("@phone"), emptySet()), writes)
    }

    @Test
    fun `toggleCollapsed prunes stale keys against the boards live column keys before persisting`() = runBlocking {
        val writes = mutableListOf<Set<String>>()
        var liveKeys = listOf("@phone", "@garden")
        val vm = viewModel(
            fetchBoardFn = { _, _, _ -> board(liveColumnKeys = liveKeys) },
            writeCollapsedFn = { writes.add(it) },
        )
        vm.refresh("2026-08-15T12:00")
        vm.toggleCollapsed("@garden")
        assertEquals(setOf("@garden"), vm.collapsed.value)

        // @garden's last action is done -- it drops out of the live set on
        // the next board read (the way a real re-fetch would reflect it).
        liveKeys = listOf("@phone")
        vm.refresh("2026-08-15T12:00")

        // Toggling an unrelated key must prune @garden's now-dead entry
        // rather than carry it forward forever.
        vm.toggleCollapsed("@phone")

        assertEquals(setOf("@phone"), vm.collapsed.value)
        assertEquals(setOf("@phone"), writes.last())
    }

    @Test
    fun `toggleCollapsed never prunes a key the live filter is merely hiding`() = runBlocking {
        // live_column_keys is computed pre-facet on the Rust side
        // (build_now_board's own doc); this pins the Kotlin side's half of
        // that contract -- a key present in liveColumnKeys survives a
        // prune even though the *rendered* columns (post-facet) might not
        // currently include it.
        val vm = viewModel(
            fetchBoardFn = { _, _, _ -> board(liveColumnKeys = listOf("@phone", "@computer")) },
        )
        vm.refresh("2026-08-15T12:00")

        vm.toggleCollapsed("@computer")
        vm.toggleCollapsed("@phone")

        assertEquals(setOf("@computer", "@phone"), vm.collapsed.value)
    }

    @Test
    fun `toggleExpanded adds then removes a key without persisting it`() = runBlocking {
        var collapsedWrites = 0
        val vm = viewModel(writeCollapsedFn = { collapsedWrites++ })

        vm.toggleExpanded("@phone")
        assertEquals(setOf("@phone"), vm.expanded.value)

        vm.toggleExpanded("@phone")
        assertEquals(emptySet<String>(), vm.expanded.value)
        assertEquals(0, collapsedWrites)
    }

    @Test
    fun `the blocked section rides on the board unchanged`() = runBlocking {
        val blockedBoard = NowBoardRecord(
            columns = emptyList(),
            blocked = listOf(
                uniffi.hummingbird_ffi_mobile.NowBlockedEntryRecord(
                    item = record("blocked-1"),
                    blockedByTitles = listOf("Ship the release"),
                ),
            ),
            contexts = emptyList(),
            liveColumnKeys = emptyList(),
            shownCount = 0u,
            totalCount = 0u,
        )
        val vm = viewModel(fetchBoardFn = { _, _, _ -> blockedBoard })

        vm.refresh("2026-08-15T12:00")

        assertEquals(1, vm.board.value!!.blocked.size)
        assertEquals("blocked-1", vm.board.value!!.blocked[0].item.id)
        assertEquals(listOf("Ship the release"), vm.board.value!!.blocked[0].blockedByTitles)
    }

    // ------------------------------------------------------ panes (#537)

    @Test
    fun `loadPanes resolves the zone queries then ranks against the resolved facts`() = runBlocking {
        val queries = listOf(MobileZoneQuery.CivilDate(zone = "device-local", atMs = 1_000L))
        var seenNowMsForQueries: Long? = null
        var seenNowMsForRank: Long? = null
        var seenFacts: List<MobileZoneFact>? = null
        val vm = viewModel(
            paneZoneQueriesFn = { nowMs -> seenNowMsForQueries = nowMs; queries },
            rankPanesFn = { nowMs, facts ->
                seenNowMsForRank = nowMs
                seenFacts = facts
                listOf(pane(MobileStandingQuestion.WASTE))
            },
        )

        vm.loadPanes(1_000L)

        assertEquals(1_000L, seenNowMsForQueries)
        assertEquals(1_000L, seenNowMsForRank)
        // The resolved facts are whatever `ZoneBridge.resolve` answers for
        // the given queries — this pins that `loadPanes` really threads
        // `paneZoneQueriesFn`'s own output through the resolve leg rather
        // than an empty list, without re-testing `ZoneBridge` itself here
        // (`ZoneBridgeTest`'s own job).
        assertEquals(1, seenFacts?.size)
        assertEquals(listOf(MobileStandingQuestion.WASTE), vm.panes.value.map { it.standingQuestion })
    }

    @Test
    fun `loadPanes replaces the previous list rather than appending`() = runBlocking {
        var call = 0
        val vm = viewModel(
            rankPanesFn = { _, _ ->
                call += 1
                if (call == 1) listOf(pane(MobileStandingQuestion.WASTE)) else listOf(pane(MobileStandingQuestion.RACE))
            },
        )

        vm.loadPanes(1_000L)
        vm.loadPanes(2_000L)

        assertEquals(listOf(MobileStandingQuestion.RACE), vm.panes.value.map { it.standingQuestion })
    }

    @Test
    fun `setScheduledDate calls the injected write fn with the item, date and clock`() = runBlocking {
        var seenItemId: String? = null
        var seenDate: String? = null
        var seenNowMs: Long? = null
        val vm = viewModel(
            setScheduledDateFn = { itemId, date, nowMs ->
                seenItemId = itemId
                seenDate = date
                seenNowMs = nowMs
            },
        )

        vm.setScheduledDate("item-1", "2026-08-15", 2_000L)

        assertEquals("item-1", seenItemId)
        assertEquals("2026-08-15", seenDate)
        assertEquals(2_000L, seenNowMs)
    }

    @Test
    fun `setScheduledDate reloads the panes after writing, reflecting the mutation immediately`() = runBlocking {
        var written = false
        val vm = viewModel(
            setScheduledDateFn = { _, _, _ -> written = true },
            rankPanesFn = { _, _ ->
                if (written) listOf(pane(MobileStandingQuestion.WEEKEND, MobilePaneBand.LIVE)) else emptyList()
            },
        )

        vm.setScheduledDate("item-1", "2026-08-15", 2_000L)

        assertEquals(listOf(MobileStandingQuestion.WEEKEND), vm.panes.value.map { it.standingQuestion })
    }

    @Test
    fun `setScheduledDate with a null date is passed straight through as a clear`() = runBlocking {
        var seenDate: String? = "unset"
        val vm = viewModel(setScheduledDateFn = { _, date, _ -> seenDate = date })

        vm.setScheduledDate("item-1", null, 2_000L)

        assertEquals(null, seenDate)
    }

    @Test
    fun `toggleFacet is a pure add-remove -- toggling twice restores the original selection`() {
        val base = FrontierFacetSelection()
        val once = base.toggled(FrontierFacet.SIZE, "deep")
        assertTrue(once.size.contains("deep"))
        assertTrue("toggled() must not mutate the receiver", base.size.isEmpty())

        val twice = once.toggled(FrontierFacet.SIZE, "deep")
        assertEquals(base, twice)
    }
}
