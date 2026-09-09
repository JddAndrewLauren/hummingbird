package net.twinion.hummingbird

import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performTouchInput
import kotlinx.coroutines.runBlocking
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

/** The wide board's drag, driven end to end (#801, ADR-0021 decision 9).
 *
 * `AxisRowWrappingTest`'s rig — NATIVE graphics, sdk 35, a qualifier wide
 * enough for the lanes to pack — and its discipline: bounds and callbacks,
 * never `captureToImage()`, which times out under Robolectric even in
 * NATIVE mode. What is checked is the thread this file's own structural
 * twin cannot see: that a long press plus a move plus a lift reaches
 * `onMoveItem` with the column the finger ended over, and that a column the
 * core did not offer is not a place a card can land.
 *
 * The spring itself is not asserted. It is a physical quality, and the only
 * place to judge it is a real tablet — which `client/android/README.md`
 * records as this slice's UI evidence, the same bargain every other Compose
 * surface here makes.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(
    sdk = [35],
    // Wide enough for the lanes to pack side by side — the gesture only
    // exists on a window this shape.
    qualifiers = "w1024dp-h800dp",
    // The stock Application: `HummingbirdApp.onCreate` schedules the hourly
    // sync through WorkManager, which is not initialized in a JVM suite —
    // `AxisRowWrappingTest`'s own escape, for the same reason.
    application = android.app.Application::class,
)
class BoardDragGestureTest {

    @get:Rule
    val rule = createComposeRule()

    private fun record(id: String, title: String) = NowItemRecord(
        id = id,
        title = title,
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

    private val board = NowBoardRecord(
        columns = listOf(
            NowColumnRecord(value = "@phone", label = "@phone", items = listOf(record("i1", "Call the plumber"))),
            NowColumnRecord(value = "@desk", label = "@desk", items = listOf(record("i2", "Write the letter"))),
        ),
        blocked = emptyList(),
        contexts = emptyList(),
        liveColumnKeys = listOf("@phone", "@desk"),
        shownCount = 2u,
        totalCount = 2u,
    )

    private fun mount(droppable: List<String>, moved: MutableList<Pair<String, String>>) {
        rule.setContent {
            HummingbirdTheme(darkTheme = false) {
                FrontierLaneBoard(
                    board = board,
                    axis = MobileFrontierAxis.CONTEXT,
                    collapsed = emptySet(),
                    expanded = emptySet(),
                    selectedId = null,
                    dark = false,
                    syncTick = 0,
                    boardWidthDp = 1024,
                    onToggleCollapsed = {},
                    onToggleExpanded = {},
                    onSelect = {},
                    onComplete = {},
                    onCloseItem = {},
                    onGrill = {},
                    onMutated = {},
                    onSubmitted = {},
                    onDroppableColumns = { _, _ -> droppable },
                    onMoveItem = { id, key -> moved += id to key },
                )
            }
        }
        rule.waitForIdle()
    }

    /** A long press on `title`'s card, a drag onto `onto`'s column, a lift.
     * The target is taken from the heading's own box, which is inside the
     * column it names. */
    private fun dragOnto(title: String, onto: String) {
        val from = rule.onNodeWithText(title).fetchSemanticsNode().boundsInRoot
        val to = rule.onNodeWithText(onto).fetchSemanticsNode().boundsInRoot
        rule.onNodeWithText(title).performTouchInput {
            down(center)
            // Past the long-press threshold, then across the board.
            advanceEventTime(1_000)
            moveBy(androidx.compose.ui.geometry.Offset(to.center.x - from.center.x, to.center.y - from.center.y))
            advanceEventTime(64)
            up()
        }
        rule.waitForIdle()
    }

    @Test
    fun `a card carried into another column reports that column`() = runBlocking {
        val moved = mutableListOf<Pair<String, String>>()
        mount(droppable = listOf("@desk"), moved = moved)

        dragOnto("Call the plumber", "@desk")

        assertEquals(listOf("i1" to "@desk"), moved)
    }

    @Test
    fun `a column the core did not offer is not a place the card can land`() = runBlocking {
        val moved = mutableListOf<Pair<String, String>>()
        // The refusal ADR-0021 decision 9 makes physical: nothing lights,
        // and the card springs home rather than being written and rejected.
        mount(droppable = emptyList(), moved = moved)

        dragOnto("Call the plumber", "@desk")

        assertTrue("a refused column must write nothing: $moved", moved.isEmpty())
    }
}
