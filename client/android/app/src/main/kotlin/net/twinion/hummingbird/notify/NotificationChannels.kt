package net.twinion.hummingbird.notify

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context

// The two channels the alert lane rings on (M2/#141, ADR-0012). The server
// decides the tier and puts the routing in the payload — `data.channel_id`
// is `"urgent"` or `"normal"`, written by `authority/src/fcm.rs`'s
// `message_json` — and this file is the device-side half of that contract.
//
// Two things about it are load-bearing:
//
// - **The ids must byte-match the payload.** `NotificationCompat.Builder`
//   takes a channel id as a plain string; posting against an id no channel
//   was created for does not throw and does not warn on a released build —
//   the notification simply lands on whatever the manager falls back to,
//   with the wrong importance and no DND treatment. There is no compile-time
//   link between `fcm.rs`'s literal and this one, so [SPECS] is where the
//   pairing is asserted (`NotificationChannelSpecTest`).
// - **Importance is where transport priority stops.** A data-only message
//   carries `android.priority: high` (whether FCM wakes a dozing device)
//   and nothing about how the notification presents — there is no
//   `notification` block any more, so the old `notification_priority` lever
//   is gone. `IMPORTANCE_HIGH` on the urgent channel is what makes an
//   urgent alert heads-up instead of silent, and it is fixed at channel
//   creation: Android will not let the app raise it later.
//
// `bypassDnd` is a request, not a grant: it is inert unless the user has
// given the app notification-policy access in Settings. That is not
// something this file can fix, so `AlertsScreen`'s health row surfaces the
// ungranted case rather than letting urgent alerts silently not ring.
object NotificationChannels {

    /** One channel, as pure data — the mapping a test can read without an
     * Android framework in the loop. */
    data class ChannelSpec(
        val id: String,
        val name: String,
        val importance: Int,
        val bypassDnd: Boolean,
    )

    /** The channel ids `fcm.rs::message_json` emits, in the same order the
     * tiers are declared server-side. Adding a tier means adding a spec
     * here *and* a `channel_id` there; neither half works alone. */
    val SPECS: List<ChannelSpec> = listOf(
        ChannelSpec(
            id = "urgent",
            name = "Urgent alerts",
            importance = NotificationManager.IMPORTANCE_HIGH,
            bypassDnd = true,
        ),
        ChannelSpec(
            id = "normal",
            name = "Alerts",
            importance = NotificationManager.IMPORTANCE_DEFAULT,
            bypassDnd = false,
        ),
    )

    /** Creates (or re-asserts) every channel in [SPECS]. Idempotent by
     * design — `createNotificationChannel` on an existing id updates only
     * the fields the user has not overridden — so calling it on every app
     * start is correct, and is the only way a channel exists before the
     * first push arrives (a notification posted against a missing channel
     * is the silent failure this prevents). */
    fun ensure(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        for (spec in SPECS) {
            val channel = NotificationChannel(spec.id, spec.name, spec.importance)
            channel.setBypassDnd(spec.bypassDnd)
            manager.createNotificationChannel(channel)
        }
    }
}
