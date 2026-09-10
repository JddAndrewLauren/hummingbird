package net.twinion.hummingbird.wear.tile

import org.junit.Assert.assertEquals
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
}
