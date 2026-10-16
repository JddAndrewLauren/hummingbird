package net.twinion.hummingbird

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import net.twinion.hummingbird.ui.theme.HummingbirdTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.hummingbird_ffi_mobile.MobileFrontierAxis
import uniffi.hummingbird_ffi_mobile.MobileUrgencyBand
import uniffi.hummingbird_ffi_mobile.NowBoardRecord
import uniffi.hummingbird_ffi_mobile.NowColumnRecord
import uniffi.hummingbird_ffi_mobile.NowItemRecord

/** Where the wide board actually draws its columns (#822): the half of the
 * lane packing `FrontierLanesTest` cannot see, because it is pure numbers
 * and this is the board feeding them in and drawing what comes back.
 *
 * A column's lane is read off its heading's left edge — every heading in a
 * lane starts at the same x — and its place in the lane off the heading's
 * top. `BoardDragGestureTest`'s rig and discipline: bounds only, never
 * `captureToImage()`.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(
    sdk = [35],
    qualifiers = "w1024dp-h800dp",
    // `BoardDragGestureTest`'s escape: the real Application schedules work
    // through a WorkManager a JVM suite never initialises.
    application = android.app.Application::class,
)
class FrontierLaneBoardLayoutTest {

    @get:Rule
    val rule = createComposeRule()

    private fun column(key: String, count: Int) = NowColumnRecord(
        value = key,
        label = key,
        items = List(count) { index ->
            NowItemRecord(
                id = "$key-$index",
                title = "$key task $index",
                deadline = null,
                urgency = MobileUrgencyBand.CALM,
                priority = 0L,
                context = null,
                size = null,
                energy = null,
                availableActions = listOf("start"),
                stage = "ready",
                canMarkDone = false,
            )
        },
    )

    private fun board(vararg columns: NowColumnRecord) = NowBoardRecord(
        columns = columns.toList(),
        blocked = emptyList(),
        contexts = emptyList(),
        liveColumnKeys = columns.map { it.value ?: "" },
        shownCount = columns.sumOf { it.items.size }.toUInt(),
        totalCount = columns.sumOf { it.items.size }.toUInt(),
    )

    /** The urgency board in its usual shape: three one-card bands ahead of a
     * `calm` far past the column cap — weights `[2, 2, 2, 8]`. */
    private val urgency = board(
        column("overdue", 1),
        column("now", 1),
        column("soon", 1),
        column("calm", 29),
    )

    private var collapsed by mutableStateOf(emptySet<String>())
    private var expanded by mutableStateOf(emptySet<String>())

    private fun mount(board: NowBoardRecord, axis: MobileFrontierAxis, widthDp: Int?) {
        rule.setContent {
            HummingbirdTheme(darkTheme = false) {
                FrontierLaneBoard(
                    board = board,
                    axis = axis,
                    collapsed = collapsed,
                    expanded = expanded,
                    selectedId = null,
                    dark = false,
                    syncTick = 0,
                    boardWidthDp = widthDp,
                    onToggleCollapsed = {},
                    onToggleExpanded = {},
                    onSelect = {},
                    onComplete = {},
                    onCloseItem = {},
                    onGrill = {},
                    onMutated = {},
                    onSubmitted = {},
                )
            }
        }
        rule.waitForIdle()
    }

    private fun left(heading: String) =
        rule.onNodeWithText(heading).fetchSemanticsNode().boundsInRoot.left

    private fun top(heading: String) =
        rule.onNodeWithText(heading).fetchSemanticsNode().boundsInRoot.top

    /** Each heading's lane, as the distinct left edges numbered left to
     * right — so two columns in one lane share a number. */
    private fun laneOf(headings: List<String>): Map<String, Int> {
        val lefts = headings.associateWith { left(it) }
        val order = lefts.values.distinct().sorted()
        return lefts.mapValues { (_, x) -> order.indexOf(x) }
    }

    @Test
    fun `the urgency board stacks its sparse bands and gives calm a lane of its own`() {
        // 830dp affords three lanes; the packing reaches two, and only those
        // two are drawn. Greedy balancing drew three here — a lane per band,
        // `calm` under `overdue`.
        mount(urgency, MobileFrontierAxis.URGENCY, widthDp = 830)

        assertEquals(
            mapOf("overdue" to 0, "now" to 0, "soon" to 0, "calm" to 1),
            laneOf(listOf("overdue", "now", "soon", "calm")),
        )
        assertTrue(
            "the bands must stack top-down in severity order",
            top("overdue") < top("now") && top("now") < top("soon"),
        )
    }

    @Test
    fun `a collapse or a reveal moves no column between lanes`() {
        // Resting weights `[8, 3, 3, 3]` over two lanes. Were the weights
        // what is drawn, shutting `@desk` would weigh it 1: the share drops
        // to 5 and the in-order fill pulls `@phone` into `@desk`'s lane
        // (the greedy packer pulled `@shop` there instead).
        val context = board(
            column("@desk", 7),
            column("@phone", 2),
            column("@shop", 2),
            column("@car", 2),
        )
        val headings = listOf("@desk", "@phone", "@shop", "@car")
        mount(context, MobileFrontierAxis.CONTEXT, widthDp = 600)
        val resting = laneOf(headings)
        assertEquals(mapOf("@desk" to 0, "@phone" to 1, "@shop" to 1, "@car" to 1), resting)

        collapsed = setOf("@desk")
        rule.waitForIdle()
        assertEquals("collapsing @desk", resting, laneOf(headings))

        collapsed = emptySet()
        expanded = setOf("@desk")
        rule.waitForIdle()
        assertEquals("revealing all of @desk", resting, laneOf(headings))
    }

    @Test
    fun `an unmeasured board draws one lane per column`() {
        // The first frame, before the width is known: the pre-lanes layout,
        // not a packing — the packer alone would stack these into two.
        mount(urgency, MobileFrontierAxis.URGENCY, widthDp = null)

        assertEquals(
            mapOf("overdue" to 0, "now" to 1, "soon" to 2, "calm" to 3),
            laneOf(listOf("overdue", "now", "soon", "calm")),
        )
    }

    @Test
    fun `a single lane keeps the core's column order`() {
        mount(urgency, MobileFrontierAxis.URGENCY, widthDp = 390)

        assertEquals(
            mapOf("overdue" to 0, "now" to 0, "soon" to 0, "calm" to 0),
            laneOf(listOf("overdue", "now", "soon", "calm")),
        )
        assertTrue(
            "one lane must be the core's order, top to bottom",
            top("overdue") < top("now") && top("now") < top("soon") && top("soon") < top("calm"),
        )
    }
}
