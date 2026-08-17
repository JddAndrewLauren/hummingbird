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

    private fun spec(key: String) = NotificationChannels.SPECS.single { it.key == key }

    @Test
    fun `the tier keys are exactly the two the server can emit`() {
        assertEquals(
            listOf("urgent", "normal"),
            NotificationChannels.SPECS.map { it.key },
        )
    }

    @Test
    fun `the bypassing tier gets its own channel id once policy access is held`() {
        // The platform zeroes `bypassDnd` on a channel created without
        // notification-policy access, and neither re-creating nor
        // deleting-and-recreating that id can raise it afterwards. So the
        // granted case must resolve to an id that has never existed
        // before, or the urgent tier silently never bypasses DND.
        val urgent = spec("urgent")
        val ungranted = NotificationChannels.channelId(urgent, policyAccess = false)
        val granted = NotificationChannels.channelId(urgent, policyAccess = true)

        assertEquals("urgent", ungranted)
        assertTrue(
            "the granted generation must be a distinct id, was $granted",
            granted != ungranted,
        )
    }

    @Test
    fun `a non-bypassing tier has one id whatever the grant`() {
        // Nothing about `normal` depends on DND access, and churning its
        // id would drop the user's own tuning of that channel for no gain.
        val normal = spec("normal")
        assertEquals("normal", NotificationChannels.channelId(normal, policyAccess = false))
        assertEquals("normal", NotificationChannels.channelId(normal, policyAccess = true))
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
            assertTrue("${channel.key} has no name", channel.name.isNotBlank())
        }
    }
}
