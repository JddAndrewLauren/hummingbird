package net.twinion.hummingbird

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The accumulator behind Gmail-style chrome hiding (operator request
// 2026-08-20), as arithmetic. `ChromeScrollStructuralTest` pins the wiring
// — that the connection hangs on the one Scaffold and reads `consumed.y` —
// but the thresholds, the direction-flip reset and `reveal()` are rules
// nothing was checking: a sign error or a swapped threshold would pass
// every source gate and only show up as bars that flicker on the device.
//
// A plain JVM test, no Robolectric: `ChromeScrollState` is a Compose
// `@Stable` holder with no Android dependency, and its `NestedScrollConnection`
// is callable directly.
class ChromeScrollStateTest {

    /** One scroll frame, as the Scaffold's connection sees it: negative
     * `y` is a downward scroll (finger up, content moving up). */
    private fun ChromeScrollState.scroll(dy: Float) {
        connection.onPostScroll(
            consumed = Offset(0f, dy),
            available = Offset.Zero,
            source = NestedScrollSource.UserInput,
        )
    }

    @Test
    fun `the chrome starts visible`() {
        assertTrue(ChromeScrollState().chromeVisible)
    }

    @Test
    fun `a short scroll down leaves the chrome alone`() {
        val chrome = ChromeScrollState()

        chrome.scroll(-CHROME_HIDE_THRESHOLD_PX)

        assertTrue(
            "48px is the threshold, not a hit — hide reluctantly",
            chrome.chromeVisible,
        )
    }

    @Test
    fun `scrolling down past the hide threshold hides the chrome`() {
        val chrome = ChromeScrollState()

        chrome.scroll(-30f)
        chrome.scroll(-30f)

        assertFalse("60px of downward run must hide the chrome", chrome.chromeVisible)
    }

    @Test
    fun `scrolling back up past the show threshold reveals it again`() {
        val chrome = ChromeScrollState()
        chrome.scroll(-60f)

        chrome.scroll(CHROME_SHOW_THRESHOLD_PX + 1f)

        assertTrue("17px of upward run must bring the chrome back", chrome.chromeVisible)
    }

    @Test
    fun `the show threshold is the eager one - a 17px pull up reveals, a 17px push down does not hide`() {
        // The asymmetry IS the feel: hide reluctantly (48px), reveal
        // eagerly (16px). One test so a later edit cannot quietly make
        // them equal.
        val hiding = ChromeScrollState()
        hiding.scroll(-(CHROME_SHOW_THRESHOLD_PX + 1f))
        assertTrue("17px down must not be enough to hide", hiding.chromeVisible)

        val showing = ChromeScrollState()
        showing.scroll(-60f)
        showing.scroll(CHROME_SHOW_THRESHOLD_PX + 1f)
        assertTrue("17px up must be enough to show", showing.chromeVisible)
    }

    @Test
    fun `a direction flip resets the run rather than netting against it`() {
        val chrome = ChromeScrollState()

        // 40px down, then a small bounce up, then 40px down again: netting
        // (40 - 5 + 40 = 75) would hide, but the flip restarts the run, so
        // the second leg is only 40px and the bars hold still. That is what
        // keeps a jittery finger from strobing the chrome.
        chrome.scroll(-40f)
        chrome.scroll(5f)
        chrome.scroll(-40f)

        assertTrue("a reversal must restart the run, not accumulate through it", chrome.chromeVisible)
    }

    @Test
    fun `crossing a threshold restarts the run`() {
        val chrome = ChromeScrollState()
        chrome.scroll(-60f)
        assertFalse(chrome.chromeVisible)

        // The 12px of leftover downward travel from before is gone: the
        // accumulator zeroed at the hide, so this 17px up is a full run and
        // reveals.
        chrome.scroll(CHROME_SHOW_THRESHOLD_PX + 1f)

        assertTrue(chrome.chromeVisible)
    }

    @Test
    fun `reveal shows the chrome and drops whatever run was building`() {
        val chrome = ChromeScrollState()
        chrome.scroll(-40f)

        // A navigation: `AppRoot` reveals on every route change, and the
        // 40px already banked must not carry over and hide the bars 8px
        // into the new screen.
        chrome.reveal()
        chrome.scroll(-40f)

        assertTrue("reveal must zero the accumulator, not just flip the flag", chrome.chromeVisible)
    }

    @Test
    fun `a zero-delta frame changes nothing`() {
        val chrome = ChromeScrollState()

        // Top-of-list overscroll consumes nothing — the pull-to-refresh
        // interplay this connection is built around. A run in progress must
        // survive it untouched (neither reset by a spurious "flip" nor
        // added to).
        chrome.scroll(-40f)
        chrome.scroll(0f)
        chrome.scroll(-9f)

        assertFalse("49px of run across a no-op frame must still hide", chrome.chromeVisible)
    }

    @Test
    fun `the connection consumes nothing itself`() {
        // It observes; the scrolling child keeps every pixel. Returning
        // anything else would steal scroll from the list under it.
        val consumed = ChromeScrollState().connection.onPostScroll(
            consumed = Offset(0f, -100f),
            available = Offset.Zero,
            source = NestedScrollSource.UserInput,
        )

        assertTrue(consumed == Offset.Zero)
    }
}
