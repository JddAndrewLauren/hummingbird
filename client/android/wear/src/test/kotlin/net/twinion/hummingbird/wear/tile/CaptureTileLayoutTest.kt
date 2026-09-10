package net.twinion.hummingbird.wear.tile

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// The tile's tap, pinned (ADR-0039): a `LaunchAction` that names the
// phone-shared package and the watch's capture activity by their exact
// strings. Both are literals on purpose (`CaptureTileLayout.kt`'s header),
// and both are the kind a rename breaks without a compile error.
class CaptureTileLayoutTest {

    @Test
    fun `the tile launches CaptureActivity in this package`() {
        val activity = captureLaunchAction().androidActivity ?: error("no AndroidActivity on the launch action")
        assertEquals("net.twinion.hummingbird", activity.packageName)
        assertEquals("net.twinion.hummingbird.wear.capture.CaptureActivity", activity.className)
    }

    /** The two glyph rounds land in `MainActivity` with the list they want
     * under the key it reads — restated here as a string, so this pins
     * the two spellings agree and that the value is one the activity
     * accepts. */
    @Test
    fun `the glyph rounds launch MainActivity with the route it reads`() {
        for (route in listOf(ROUTE_ITEMS, ROUTE_QUESTIONS)) {
            val activity = routeLaunchAction(route).androidActivity ?: error("no AndroidActivity on the route action")
            assertEquals("net.twinion.hummingbird", activity.packageName)
            assertEquals("net.twinion.hummingbird.wear.MainActivity", activity.className)
            val extra = activity.keyToExtraMapping[net.twinion.hummingbird.wear.MainActivity.EXTRA_ROUTE]
                ?: error("no route extra under MainActivity.EXTRA_ROUTE")
            assertEquals(route, (extra as androidx.wear.protolayout.ActionBuilders.AndroidStringExtra).value)
            assertTrue("$route must be a route MainActivity accepts", route in net.twinion.hummingbird.wear.Routes.TILE_TARGETS)
        }
    }
}
