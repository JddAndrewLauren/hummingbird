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
// constants, the `BottomNavStructuralTest` idiom.
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

    @Test
    fun `fans the first columns across the lanes before stacking anything`() {
        // Every lane starts empty, so the fullest columns — which arrive
        // first — read left to right along the top.
        assertEquals(listOf(listOf(0), listOf(1), listOf(2)), packLanes(listOf(9, 8, 7), 3))
    }

    @Test
    fun `stacks the short columns under whichever lane is shortest`() {
        assertEquals(
            listOf(listOf(0), listOf(1, 2, 3)),
            packLanes(listOf(9, 2, 2, 2), 2),
        )
    }

    @Test
    fun `breaks a tie leftwards`() {
        assertEquals(
            listOf(listOf(0, 2), listOf(1, 3)),
            packLanes(listOf(1, 1, 1, 1), 2),
        )
    }

    @Test
    fun `preserves the given order outright in a single lane`() {
        assertEquals(listOf(listOf(0, 1, 2)), packLanes(listOf(1, 9, 3), 1))
    }

    @Test
    fun `returns one lane per column when asked for exactly that`() {
        assertEquals(
            listOf(listOf(0), listOf(1), listOf(2)),
            packLanes(listOf(5, 1, 1), 3),
        )
    }

    @Test
    fun `returns no lanes for no lanes, whatever the weights`() {
        assertEquals(emptyList<List<Int>>(), packLanes(listOf(1, 2), 0))
        assertEquals(
            listOf(emptyList<Int>(), emptyList(), emptyList()),
            packLanes(emptyList(), 3),
        )
    }

    // ------------------------------------------------------- the parity pin

    @Test
    fun `the web parity source still carries the constants this port copies`() {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
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
    }
}
