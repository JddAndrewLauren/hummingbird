package net.twinion.hummingbird.notify

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context

// The two channels the alert lane rings on (M2/#141, ADR-0012). The server
// decides the tier and puts the routing in the payload — `data.channel_id`
// is `"urgent"` or `"normal"`, written by `authority/src/fcm.rs`'s
// `message_json` — and this file is the device-side half of that contract.
//
// Three things about it are load-bearing:
//
// - **The payload keys must byte-match the server's.** There is no
//   compile-time link between `fcm.rs`'s literal and this one, so [SPECS]
//   is where the pairing is asserted (`NotificationChannelSpecTest`). What
//   the payload names is the *tier* ([ChannelSpec.key]); the channel id it
//   currently resolves to is [channelId]'s answer, and only [AlertNotifier]
//   needs it.
// - **Importance is where transport priority stops.** A data-only message
//   carries `android.priority: high` (whether FCM wakes a dozing device)
//   and nothing about how the notification presents — there is no
//   `notification` block any more, so the old `notification_priority` lever
//   is gone. `IMPORTANCE_HIGH` on the urgent channel is what makes an
//   urgent alert heads-up instead of silent, and it is fixed at channel
//   creation: Android will not let the app raise it later.
// - **`bypassDnd` is applied at creation, or never.** The platform zeroes
//   the flag on a channel created while the app lacks notification-policy
//   access, and a later `createNotificationChannel` on an existing id
//   cannot raise it again (only name, description and group are updatable;
//   importance only downward). Deleting and recreating the *same* id does
//   not help either — the platform restores a recreated channel's previous
//   settings, which is the whole trap. So the grant is part of the
//   channel's identity here: a bypassing tier resolves to a distinct id
//   ([DND_SUFFIX]) that is only ever created while access is held, and
//   [ensure] retires the other generation when it flips. The user's grant
//   still has to arrive from Settings — `AlertsScreen`'s health row is the
//   route — but from this file's side it now lands rather than being inert.
object NotificationChannels {

    /** One channel, as pure data — the mapping a test can read without an
     * Android framework in the loop. */
    data class ChannelSpec(
        /** The `data.channel_id` value the server sends for this tier. A
         * key, not the channel id: see [channelId]. */
        val key: String,
        val name: String,
        val importance: Int,
        val bypassDnd: Boolean,
    )

    /** The tier keys `fcm.rs::message_json` emits, in the same order the
     * tiers are declared server-side. Adding a tier means adding a spec
     * here *and* a `channel_id` there; neither half works alone. */
    val SPECS: List<ChannelSpec> = listOf(
        ChannelSpec(
            key = "urgent",
            name = "Urgent alerts",
            importance = NotificationManager.IMPORTANCE_HIGH,
            bypassDnd = true,
        ),
        ChannelSpec(
            key = "normal",
            name = "Alerts",
            importance = NotificationManager.IMPORTANCE_DEFAULT,
            bypassDnd = false,
        ),
    )

    /** The generation suffix of a bypassing channel created *with* policy
     * access. Never sent over the wire — the payload only ever carries a
     * key from [SPECS]. */
    private const val DND_SUFFIX = ".dnd"

    /** The channel id a tier resolves to, as a pure function of the spec
     * and whether the app currently holds notification-policy access. A
     * non-bypassing tier has one id forever; the urgent tier has two, and
     * which one exists is exactly which one the platform will honour. */
    fun channelId(spec: ChannelSpec, policyAccess: Boolean): String =
        if (spec.bypassDnd && policyAccess) spec.key + DND_SUFFIX else spec.key

    /** The live channel id for a payload's `channel_id` key, or null if
     * this build knows no such tier. Reads the grant at call time, so a
     * notification posted after [ensure] has run against a fresh grant
     * lands on the channel that actually bypasses. */
    fun channelIdFor(context: Context, key: String): String? {
        val spec = SPECS.firstOrNull { it.key == key } ?: return null
        val manager = context.getSystemService(NotificationManager::class.java)
        return channelId(spec, manager?.isNotificationPolicyAccessGranted == true)
    }

    /** Creates (or re-asserts) every channel in [SPECS], and retires the
     * generation the current grant makes wrong.
     *
     * Idempotent, so calling it on every app start is correct — and it is
     * the only way a channel exists before the first push arrives (a
     * notification posted against a missing channel is the silent failure
     * this prevents). It must also run on **return from Settings**, since
     * granting policy access is what makes the bypassing generation
     * creatable; `MainActivity` calls it on every resume for that reason.
     *
     * Retiring the stale generation cancels any notification still posted
     * against it. That is one urgent ring lost, once, at the moment the
     * user grants or revokes DND access — the alert itself is untouched and
     * still shows on the alerts surface, and the alternative is two
     * near-identical "Urgent alerts" rows in Settings for the app's life.
     */
    fun ensure(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        val policyAccess = manager.isNotificationPolicyAccessGranted
        for (spec in SPECS) {
            val id = channelId(spec, policyAccess)
            val channel = NotificationChannel(id, spec.name, spec.importance)
            channel.setBypassDnd(spec.bypassDnd)
            manager.createNotificationChannel(channel)

            val stale = channelId(spec, !policyAccess)
            if (stale != id) manager.deleteNotificationChannel(stale)
        }
    }
}
