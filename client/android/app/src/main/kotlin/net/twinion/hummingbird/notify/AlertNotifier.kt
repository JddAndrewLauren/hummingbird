package net.twinion.hummingbird.notify

import android.app.Notification
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import net.twinion.hummingbird.MainActivity
import net.twinion.hummingbird.brand.R
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
//   [EXTRA_ALERT_ID] — plus [EXTRA_SOURCE]/[EXTRA_SOURCE_KEY], which say
//   what the alert is about (ADR-0027); `MainActivity` reads the extras,
//   asks the core where they lead, and navigates.
// - **A thread is a group, and its summary is the app's to retire** (#922,
//   ADR-0042). Alerts sharing a `thread` post with `setGroup`, beside one
//   summary; Android does not remove a summary when the app cancels its last
//   child, so [cancel] and [post] both sweep the orphans ([ThreadGroups]).
// - **No delete intent, deliberately.** Swiping the notification away does
//   nothing to the alert (ADR-0012: dismissing a *notification* is not an
//   Ack, and inferring one would silently settle alerts the user only
//   brushed off a lock screen). The Ack action is the only gesture that
//   settles anything, and it is explicit.
object AlertNotifier {

    /** The notification id every alert shares — uniqueness comes from the
     * tag (see the note above). */
    const val NOTIFICATION_ID = 1

    /** The id every thread summary shares; uniqueness comes from its tag
     * ([ThreadGroups.summaryTag]), exactly as an alert's does. */
    private const val SUMMARY_ID = 2

    /** The alert id, carried on the tap intent into `MainActivity` and on
     * the Ack broadcast into [AckReceiver]. */
    const val EXTRA_ALERT_ID = "net.twinion.hummingbird.extra.ALERT_ID"

    /** What the alert is about, carried on the tap intent alongside the id
     * (ADR-0027). **The extras decide the destination; the data URI never
     * does** — the URI exists only to keep two alerts' `PendingIntent`s
     * distinct (see below), and encoding navigation in it would be a
     * second encoding of the same fact, which the structural test forbids. */
    const val EXTRA_SOURCE = "net.twinion.hummingbird.extra.SOURCE"
    const val EXTRA_SOURCE_KEY = "net.twinion.hummingbird.extra.SOURCE_KEY"

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
            .putExtra(EXTRA_SOURCE, alert.source)
            .putExtra(EXTRA_SOURCE_KEY, alert.sourceKey)
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

        // The payload names a tier; which channel that tier currently
        // resolves to depends on whether the app holds DND-access, and the
        // answer can change between two arrivals of the same alert (see
        // NotificationChannels). Falling back to the key itself keeps a
        // notification postable even if `ensure` has somehow not run.
        val channelId = NotificationChannels.channelIdFor(app, alert.channelKey)
            ?: alert.channelKey

        val builder = NotificationCompat.Builder(app, channelId)
            // The source's own glyph, the bell for a slug this build does
            // not know (#922, ADR-0042).
            .setSmallIcon(SourceIcons.drawableFor(alert.sourceIcon))
            .setContentTitle(alert.title)
            .setContentIntent(tap)
            .setAutoCancel(true)
            .setCategory(NotificationCompat.CATEGORY_REMINDER)
            // "Ack", not "Dismiss": they mean different things in this
            // domain (design README's exact-domain-words rule), and this
            // one settles the alert on the authority.
            .addAction(R.drawable.ic_alert, "Ack", ack)
        // "<source name> · <context>" in the header; absent when the payload
        // names no source (a server older than #922), never an empty string.
        alert.subText?.let { builder.setSubText(it) }
        // Alerts sharing a thread stack under one summary, keyed by the
        // thread the server already scoped to the source. No thread, no
        // group: the notification stands alone, as before.
        alert.thread?.let { builder.setGroup(it) }
        // Absent body stays absent — never `setContentText("")`, which
        // renders a blank second line rather than a one-line notification.
        alert.body?.let {
            builder.setContentText(it)
            builder.setStyle(NotificationCompat.BigTextStyle().bigText(it))
        }

        val manager = NotificationManagerCompat.from(app)
        manager.notify(alert.alertId, NOTIFICATION_ID, builder.build())
        alert.thread?.let { postSummary(app, manager, alert, it, channelId) }
        removeOrphanedSummaries(manager, exceptTag = null)
    }

    /** The one summary a thread's notifications collapse under. Re-posting it
     * on every child keeps it in step with the newest alert's wording. It
     * alerts nothing itself (`GROUP_ALERT_CHILDREN`), so a thread's first
     * alert rings once, not twice. */
    private fun postSummary(
        app: Context,
        manager: NotificationManagerCompat,
        alert: AlertNotification,
        thread: String,
        channelId: String,
    ) {
        val open = PendingIntent.getActivity(
            app,
            0,
            Intent(app, MainActivity::class.java)
                .setAction(Intent.ACTION_VIEW)
                .setData(android.net.Uri.parse("hummingbird://thread/${android.net.Uri.encode(thread)}"))
                .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val summary = NotificationCompat.Builder(app, channelId)
            .setSmallIcon(SourceIcons.drawableFor(alert.sourceIcon))
            .setContentTitle(alert.context ?: alert.sourceName ?: alert.title)
            .setContentIntent(open)
            .setAutoCancel(true)
            .setGroup(thread)
            .setGroupSummary(true)
            .setGroupAlertBehavior(NotificationCompat.GROUP_ALERT_CHILDREN)
        alert.subText?.let { summary.setSubText(it) }
        manager.notify(ThreadGroups.summaryTag(thread), SUMMARY_ID, summary.build())
    }

    /** Cancels every thread summary with no alert left under it. */
    private fun removeOrphanedSummaries(manager: NotificationManagerCompat, exceptTag: String?) {
        val active = manager.activeNotifications
            // The platform may still list the notification this call has just
            // cancelled; it is gone as far as the group is concerned.
            .filter { it.tag != exceptTag }
            .map {
                ThreadGroups.Posted(
                    tag = it.tag,
                    id = it.id,
                    group = it.notification.group,
                    isSummary = it.notification.flags and Notification.FLAG_GROUP_SUMMARY != 0,
                )
            }
        for (orphan in ThreadGroups.orphanedSummaries(active)) {
            manager.cancel(orphan.tag, orphan.id)
        }
    }

    /** Retires one alert's notification — what a completed Ack does, so the
     * ring does not outlive the thing it was about. */
    fun cancel(context: Context, alertId: String) {
        val manager = NotificationManagerCompat.from(context.applicationContext)
        manager.cancel(alertId, NOTIFICATION_ID)
        // Its thread's summary goes with its last alert (#922, ADR-0042).
        removeOrphanedSummaries(manager, exceptTag = alertId)
    }
}
