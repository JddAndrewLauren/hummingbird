package net.twinion.hummingbird.notify

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import net.twinion.hummingbird.MainActivity
import net.twinion.hummingbird.R
import net.twinion.hummingbird.push.AckReceiver

// Posting and cancelling one alert's notification (M2/#141, ADR-0012).
//
// Three decisions are worth stating, because none of them is recoverable
// once shipped wrong:
//
// - **Tag, not a unique id.** Every alert posts as `notify(tag = alertId,
//   id = NOTIFICATION_ID)`. A re-sent alert then *replaces* its own
//   notification in place rather than stacking a second copy, and
//   [cancel] can retire exactly one alert without touching the others.
//   Since `AlertNotification` is byte-stable by construction (no clock in
//   the mapper), a replace is visually silent — which is the whole point.
// - **The tap is an Activity intent, not a trampoline.** Android 12+ bans
//   starting an Activity from a broadcast receiver or service woken by a
//   notification tap, so the tap goes straight to [MainActivity] carrying
//   [EXTRA_ALERT_ID]; `MainActivity` reads the extra and navigates.
// - **No delete intent, deliberately.** Swiping the notification away does
//   nothing to the alert (ADR-0012: dismissing a *notification* is not an
//   Ack, and inferring one would silently settle alerts the user only
//   brushed off a lock screen). The Ack action is the only gesture that
//   settles anything, and it is explicit.
object AlertNotifier {

    /** The notification id every alert shares — uniqueness comes from the
     * tag (see the note above). */
    const val NOTIFICATION_ID = 1

    /** The alert id, carried on the tap intent into `MainActivity` and on
     * the Ack broadcast into [AckReceiver]. */
    const val EXTRA_ALERT_ID = "net.twinion.hummingbird.extra.ALERT_ID"

    fun post(context: Context, alert: AlertNotification) {
        val app = context.applicationContext

        val tapIntent = Intent(app, MainActivity::class.java)
            .setAction(Intent.ACTION_VIEW)
            // The alert id rides in the data uri as well as the extra:
            // `PendingIntent` equality ignores extras, so two alerts would
            // otherwise share one PendingIntent and the second tap would
            // open the first alert.
            .setData(android.net.Uri.parse("hummingbird://alert/${alert.alertId}"))
            .putExtra(EXTRA_ALERT_ID, alert.alertId)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        val tap = PendingIntent.getActivity(
            app,
            0,
            tapIntent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        val ackIntent = Intent(app, AckReceiver::class.java)
            .setAction(AckReceiver.ACTION_ACK)
            .setData(android.net.Uri.parse("hummingbird://ack/${alert.alertId}"))
            .putExtra(EXTRA_ALERT_ID, alert.alertId)
        val ack = PendingIntent.getBroadcast(
            app,
            0,
            ackIntent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        val builder = NotificationCompat.Builder(app, alert.channelId)
            .setSmallIcon(R.drawable.ic_alert)
            .setContentTitle(alert.title)
            .setContentIntent(tap)
            .setAutoCancel(true)
            .setCategory(NotificationCompat.CATEGORY_REMINDER)
            // "Ack", not "Dismiss": they mean different things in this
            // domain (design README's exact-domain-words rule), and this
            // one settles the alert on the authority.
            .addAction(R.drawable.ic_alert, "Ack", ack)
        // Absent body stays absent — never `setContentText("")`, which
        // renders a blank second line rather than a one-line notification.
        alert.body?.let {
            builder.setContentText(it)
            builder.setStyle(NotificationCompat.BigTextStyle().bigText(it))
        }

        NotificationManagerCompat.from(app)
            .notify(alert.alertId, NOTIFICATION_ID, builder.build())
    }

    /** Retires one alert's notification — what a completed Ack does, so the
     * ring does not outlive the thing it was about. */
    fun cancel(context: Context, alertId: String) {
        NotificationManagerCompat.from(context.applicationContext)
            .cancel(alertId, NOTIFICATION_ID)
    }
}
