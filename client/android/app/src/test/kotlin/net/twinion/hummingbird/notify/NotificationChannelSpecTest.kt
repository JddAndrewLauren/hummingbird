package net.twinion.hummingbird.notify

import android.app.NotificationManager
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// The device half of the `data.channel_id` contract (M2/#141): the server
// writes `"urgent"`/`"normal"` in `authority/src/fcm.rs::message_json`, and
// a notification posted against an id no channel exists for is silently
// re-homed rather than rejected. Nothing links the two literals at compile
// time, so this test is the link — the ids, and the importance each tier's
// ring depends on now that the payload carries no `notification` block to
// set `notification_priority` from.
class NotificationChannelSpecTest {

    private fun spec(id: String) = NotificationChannels.SPECS.single { it.id == id }

    @Test
    fun `the channel ids are exactly the two the server can emit`() {
        assertEquals(
            listOf("urgent", "normal"),
            NotificationChannels.SPECS.map { it.id },
        )
    }

    @Test
    fun `urgent is high importance and asks to bypass DND`() {
        val urgent = spec("urgent")
        // IMPORTANCE_HIGH is what makes an urgent alert heads-up; it is
        // fixed at creation and cannot be raised afterwards.
        assertEquals(NotificationManager.IMPORTANCE_HIGH, urgent.importance)
        assertTrue("urgent must request DND bypass", urgent.bypassDnd)
    }

    @Test
    fun `normal is default importance and does not ask to bypass DND`() {
        val normal = spec("normal")
        assertEquals(NotificationManager.IMPORTANCE_DEFAULT, normal.importance)
        assertTrue("normal must not request DND bypass", !normal.bypassDnd)
    }

    @Test
    fun `every channel carries a user-visible name`() {
        // The name is what Settings shows; an empty one leaves the user
        // unable to tell the two channels apart when tuning them.
        for (channel in NotificationChannels.SPECS) {
            assertTrue("${channel.id} has no name", channel.name.isNotBlank())
        }
    }
}
