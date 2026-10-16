package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// `FrontierLanes.kt` is a verbatim port of the web's `frontier-lanes.ts`,
// and these are that file's own tests (`frontier-lanes.test.ts`), mirrored
// case for case so the two implementations cannot quietly diverge on a
// boundary. The last test is the drift gate the mirroring depends on: it
// reads the web source and pins LANE_MIN/GAP equal to this port's
// constants, and the packing and weighing rules as the web writes them,
// the `BottomNavStructuralTest` idiom.
class FrontierLanesTest {

    // ------------------------------------------------------- laneCountFor

    @Test
    fun `gives every column its own lane when the width is unknown`() {
        // The pre-lanes layout, and the only honest answer for an
        // unmeasured first frame.
        assertEquals(5, laneCountFor(null, 5))
        assertEquals(1, laneCountFor(null, 1))
    }

    @Test
    fun `has no lanes when there are no columns, measured or not`() {
        assertEquals(0, laneCountFor(null, 0))
        assertEquals(0, laneCountFor(1200, 0))
    }

    @Test
    fun `fits as many 240dp lanes as the width and its gaps allow`() {
        // n lanes cost n*240 + (n-1)*24.
        assertEquals(1, laneCountFor(240, 9))
        assertEquals(1, laneCountFor(503, 9))
        assertEquals(2, laneCountFor(504, 9))
        assertEquals(2, laneCountFor(767, 9))
        assertEquals(3, laneCountFor(768, 9))
        assertEquals(4, laneCountFor(1032, 9))
    }

    @Test
    fun `never opens a lane it has no column for`() {
        assertEquals(2, laneCountFor(1600, 2))
    }

    @Test
    fun `keeps one lane where nothing fits the minimum`() {
        // A floor rather than zero: below the minimum the columns still
        // have to be drawn somewhere.
        assertEquals(1, laneCountFor(390, 4))
        assertEquals(1, laneCountFor(1, 4))
    }

    // ---------------------------------------------------------- packLanes

    private fun lanes(vararg lanes: List<Int>): List<List<Int>> = lanes.toList()

    @Test
    fun `fans the fullest columns across the lanes before stacking anything`() {
        // A heavy column carries its whole share on its own, so the next
        // one starts a fresh lane.
        assertEquals(lanes(listOf(0), listOf(1), listOf(2)), packLanes(listOf(9, 8, 7), 3))
    }

    @Test
    fun `stacks the short columns under the lane already open`() {
        assertEquals(lanes(listOf(0), listOf(1, 2, 3)), packLanes(listOf(9, 2, 2, 2), 2))
    }

    @Test
    fun `stacks the sparse bands the urgency axis leads with`() {
        // The shape #822 is about: overdue/now/soon arrive first in severity
        // order, one card each, ahead of a capped `calm`. Greedy balancing
        // answered [[0,3],[1],[2]] here — a lane per band and `calm` under
        // the first.
        assertEquals(lanes(listOf(0, 1, 2), listOf(3)), packLanes(listOf(2, 2, 2, 8), 3))
    }

    @Test
    fun `does not swallow a column that dwarfs the ones already in the lane`() {
        assertEquals(lanes(listOf(0, 1, 2), listOf(3)), packLanes(listOf(2, 2, 2, 31), 3))
        assertEquals(lanes(listOf(0), listOf(1)), packLanes(listOf(1, 8), 2))
    }

    @Test
    fun `aims no lane lower than the tallest single column`() {
        assertEquals(lanes(listOf(0, 1, 2), listOf(3)), packLanes(listOf(2, 2, 2, 8), 3))
    }

    @Test
    fun `draws no lane the weights never reached`() {
        assertEquals(2, packLanes(listOf(2, 2, 2, 8), 3).size)
        assertEquals(lanes(listOf(0), listOf(1, 2)), packLanes(listOf(5, 1, 1), 3))
    }

    @Test
    fun `keeps a lumpy fullest-first axis on all the lanes it affords`() {
        assertEquals(
            lanes(listOf(0), listOf(1, 2), listOf(3, 4, 5)),
            packLanes(listOf(8, 5, 4, 3, 3, 8), 3),
        )
    }

    @Test
    fun `splits a run of equal columns in reading order`() {
        // Greedy interleaved these [[0,2],[1,3]]; filling reads them down
        // the left lane first.
        assertEquals(lanes(listOf(0, 1), listOf(2, 3)), packLanes(listOf(1, 1, 1, 1), 2))
    }

    @Test
    fun `preserves the given order outright in a single lane`() {
        assertEquals(lanes(listOf(0, 1, 2)), packLanes(listOf(1, 9, 3), 1))
    }

    @Test
    fun `gives every column its own lane when each fills one`() {
        assertEquals(lanes(listOf(0), listOf(1), listOf(2)), packLanes(listOf(1, 1, 1), 3))
    }

    @Test
    fun `returns no lanes for no lanes, and none for no columns`() {
        assertEquals(emptyList<List<Int>>(), packLanes(listOf(1, 2), 0))
        assertEquals(emptyList<List<Int>>(), packLanes(emptyList(), 3))
    }

    // ------------------------------------------------------- frontierLanes

    @Test
    fun `gives every column its own lane when the board is unmeasured`() {
        // Answered before the packer, as the web's `frontierLanes` does: the
        // packer itself would stack these into two lanes.
        assertEquals(
            lanes(listOf(0), listOf(1), listOf(2), listOf(3)),
            frontierLanes(listOf(2, 2, 2, 8), null),
        )
        assertEquals(emptyList<List<Int>>(), frontierLanes(emptyList(), null))
    }

    @Test
    fun `packs into the lanes the measured width affords`() {
        // Three lanes afforded at 830dp, two drawn.
        assertEquals(lanes(listOf(0, 1, 2), listOf(3)), frontierLanes(listOf(2, 2, 2, 8), 830))
        assertEquals(
            lanes(listOf(0), listOf(1, 2), listOf(3, 4, 5)),
            frontierLanes(listOf(8, 5, 4, 3, 3, 8), 830),
        )
    }

    @Test
    fun `stacks the whole board in one lane where only one fits`() {
        assertEquals(lanes(listOf(0, 1, 2, 3)), frontierLanes(listOf(2, 2, 2, 8), 390))
    }

    // ----------------------------------------------------- laneWeightsFor

    @Test
    fun `charges a heading, the cards shown, and the control if there is one`() {
        assertEquals(listOf(2, 4, 7), laneWeightsFor(listOf(1, 3, 6), 6))
        assertEquals(listOf(8, 8), laneWeightsFor(listOf(7, 29), 6))
        assertEquals(listOf(7), laneWeightsFor(listOf(6), 6))
    }

    @Test
    fun `costs an empty column its heading alone`() {
        assertEquals(listOf(1), laneWeightsFor(listOf(0), 6))
    }

    @Test
    fun `takes counts and a cap, and nothing about what is on screen`() {
        // A column's weight is its RESTING height: there is no parameter
        // for collapsed or expanded, so no toggle can repack the board.
        // `FrontierLaneBoardLayoutTest` drives the toggles themselves.
        assertEquals(listOf(2, 2, 2, 8), laneWeightsFor(listOf(1, 1, 1, 29), 6))
        assertEquals(listOf(2, 2, 2, 16), laneWeightsFor(listOf(1, 1, 1, 29), 14))
    }

    @Test
    fun `grows the cap without changing what a small column costs`() {
        assertEquals(listOf(3, 3, 3, 8), laneWeightsFor(listOf(2, 2, 2, 29), 6))
        assertEquals(listOf(3, 3, 3, 16), laneWeightsFor(listOf(2, 2, 2, 29), 14))
    }

    // ------------------------------------------------------- the parity pin

    @Test
    fun `the web parity source still carries the constants this port copies`() {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see client/android/build.gradle.kts)")
        val file = File(root, "client/web/src/screens/frontier-lanes.ts")
        check(file.isFile) { "frontier-lanes.ts not found under $root" }
        val web = file.readText()
        assertTrue(
            "frontier-lanes.ts must still say LANE_MIN = $LANE_MIN_DP — if the web " +
                "changed its lane minimum, port the change here rather than letting " +
                "the two boards pack differently",
            web.contains("const LANE_MIN = $LANE_MIN_DP;"),
        )
        assertTrue(
            "frontier-lanes.ts must still say GAP = $LANE_GAP_DP — same porting rule",
            web.contains("const GAP = $LANE_GAP_DP;"),
        )
        // The rule itself, not only its constants: if the web stops filling
        // in order or starts weighing what is drawn, this port must follow.
        assertTrue(
            "frontier-lanes.ts's packLanes must still open a lane only past the share — " +
                "if the web changed its packing rule, port it here",
            web.contains("if (carried > 0 && carried + weight > target && lanes.length < count) {"),
        )
        assertTrue(
            "frontier-lanes.ts's laneWeightsFor must still weigh resting height alone — " +
                "same porting rule",
            web.contains("(count) => 1 + Math.min(count, columnCap) + (count > columnCap ? 1 : 0),"),
        )
    }
}
