package net.twinion.hummingbird

import android.content.Intent
import net.twinion.hummingbird.notify.AlertNotifier
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** How a tapped notification is read off its intent (#922 adds the thread
 * summary's). */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = android.app.Application::class)
class ArrivalTest {

    @Test
    fun `a thread summary's tap arrives at the Alerts screen`() {
        val intent = Intent().putExtra(AlertNotifier.EXTRA_OPEN_ALERTS, true)
        assertEquals(Arrival.Alerts, Arrival.from(intent))
    }

    @Test
    fun `an alert's tap still names its alert`() {
        val intent = Intent()
            .putExtra(AlertNotifier.EXTRA_ALERT_ID, "a-1")
            .putExtra(AlertNotifier.EXTRA_SOURCE, "auk/v1")
        assertEquals(
            Arrival.Notification(NotificationTap("a-1", "auk/v1", "")),
            Arrival.from(intent),
        )
    }

    @Test
    fun `a launcher start arrives nowhere in particular`() {
        assertNull(Arrival.from(Intent()))
        assertNull(Arrival.from(null))
        assertNull(Arrival.from(Intent().putExtra(AlertNotifier.EXTRA_OPEN_ALERTS, false)))
    }
}
