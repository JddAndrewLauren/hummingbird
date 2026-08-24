package net.twinion.hummingbird

import net.twinion.hummingbird.ui.theme.Amber600
import net.twinion.hummingbird.ui.theme.Crimson600
import net.twinion.hummingbird.ui.theme.CrimsonDark
import net.twinion.hummingbird.ui.theme.StatusWarnFgDark
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobilePaneBand

/** The Status control's tint — the Android half of `nav-alarm.test.ts`,
 * asserting the same band→tone table on the same four bands, in both
 * scopes. */
class NavAlarmTest {

    @Test
    fun `wears no tint when nothing raises the nav`() {
        assertNull(navAlarmColor(null, dark = false))
        assertNull(navAlarmColor(null, dark = true))
    }

    @Test
    fun `live and imminent read as danger in both scopes`() {
        assertEquals(Crimson600, navAlarmColor(MobilePaneBand.LIVE, dark = false))
        assertEquals(CrimsonDark, navAlarmColor(MobilePaneBand.LIVE, dark = true))
        assertEquals(Crimson600, navAlarmColor(MobilePaneBand.IMMINENT, dark = false))
        assertEquals(CrimsonDark, navAlarmColor(MobilePaneBand.IMMINENT, dark = true))
    }

    @Test
    fun `near and distant read as warn in both scopes`() {
        assertEquals(Amber600, navAlarmColor(MobilePaneBand.NEAR, dark = false))
        assertEquals(StatusWarnFgDark, navAlarmColor(MobilePaneBand.NEAR, dark = true))
        assertEquals(Amber600, navAlarmColor(MobilePaneBand.DISTANT, dark = false))
        assertEquals(StatusWarnFgDark, navAlarmColor(MobilePaneBand.DISTANT, dark = true))
    }

    /** The core folds `dormant` away before it reaches the nav, but the
     * type permits it — and a nav that invented a third tint for it would
     * be painting "everything is fine" as a state worth noticing. */
    @Test
    fun `wears no tint for a quiet band`() {
        assertNull(navAlarmColor(MobilePaneBand.DORMANT, dark = false))
        assertNull(navAlarmColor(MobilePaneBand.DORMANT, dark = true))
    }
}
