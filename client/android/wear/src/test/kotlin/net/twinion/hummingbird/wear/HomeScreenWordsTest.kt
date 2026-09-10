package net.twinion.hummingbird.wear

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// `syncAgeLine`'s contract (ADR-0039's honesty line): silent inside the
// hour, floored hours past it, and "not synced yet" when the store has
// nothing — a fact, not an apology.
class HomeScreenWordsTest {

    private val now = 1_800_000_000_000L
    private val hour = 60L * 60L * 1000L

    @Test
    fun `never synced is said plainly`() {
        assertEquals("not synced yet", syncAgeLine(null, now))
    }

    @Test
    fun `inside the hour the line is silent`() {
        assertNull(syncAgeLine(now, now))
        assertNull(syncAgeLine(now - hour + 1, now))
    }

    @Test
    fun `past the hour the age is floored to hours`() {
        assertEquals("synced 1h ago", syncAgeLine(now - hour, now))
        assertEquals("synced 1h ago", syncAgeLine(now - 2 * hour + 1, now))
        assertEquals("synced 2h ago", syncAgeLine(now - 2 * hour, now))
        assertEquals("synced 26h ago", syncAgeLine(now - 26 * hour, now))
    }
}
