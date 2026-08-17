package net.twinion.hummingbird.notify

// One arriving FCM payload, mapped — the whole of what the device decides
// about a push before it becomes a notification (M2/#141, ADR-0012).
//
// The payload is **data-only** (`authority/src/fcm.rs::message_json`, and
// its test asserting no `notification` block exists): the client builds
// every notification itself, because an FCM-rendered one never reaches
// `onMessageReceived` and so could carry no Ack action. The keys are all
// strings — `alert_id`, `severity`, `tier`, `channel_id`, `title`, and
// `body` **only when there is one** (`data` has no null, and a bodyless
// notification renders differently from one with a blank line, so absent
// and empty are not the same input here).
//
// This file has **no clock and no time imports**, and a structural test
// gates that. The text of a notification must be byte-identical across
// re-sends of the same alert, or a restamp posts a "new" notification
// instead of replacing the old one and the dismissal is undone every poll
// — a relative phrase like "2 hours ago" baked in here is exactly that
// defect. Anything time-shaped belongs on the screens, which re-render.
data class AlertNotification(
    val alertId: String,
    /** The payload's tier key — `"urgent"` or `"normal"`, validated
     * against [NotificationChannels.SPECS]. Not the channel id: which
     * channel the tier resolves to depends on the device's DND-access
     * grant, which is a live reading and so belongs at post time
     * ([NotificationChannels.channelIdFor]), not in this clock-free,
     * value-equal mapping. */
    val channelKey: String,
    val title: String,
    /** Null when the payload carried no `body` key at all — never coerced
     * to `""`, which would render as an empty second line. */
    val body: String?,
    /** What the alert is *about* (ADR-0027), carried through to the tap
     * intent so the destination can be decided without reading the mirror.
     *
     * Null for a payload from a server older than ADR-0027. Nothing here
     * interprets either string — `item:` is a convention with one owner in
     * `hummingbird_domain`, and the only sanctioned reader is the core's
     * `notificationTapTarget`, called at tap time. A `removePrefix` here
     * would be that convention hand-copied into Kotlin. */
    val source: String?,
    val sourceKey: String?,
) {
    companion object {
        /** The tier an unroutable payload lands on. `normal` rather than
         * `urgent`: guessing upward would let a malformed or future-tier
         * payload bypass DND, and being under-loud is the recoverable
         * failure. */
        const val FALLBACK_KEY = "normal"

        /** Maps a raw `data` map, or null if it is not an alert push.
         *
         * Null means only "there is nothing to *show*" — the caller still
         * runs a sync cycle, because the arrival of any push is itself
         * evidence the mirror is stale, whether or not this device can
         * render the thing that caused it. */
        fun from(data: Map<String, String>): AlertNotification? {
            val alertId = data["alert_id"]?.takeIf { it.isNotBlank() } ?: return null
            val title = data["title"]?.takeIf { it.isNotBlank() } ?: return null
            val channelKey = data["channel_id"]
                ?.takeIf { key -> NotificationChannels.SPECS.any { it.key == key } }
                ?: FALLBACK_KEY
            return AlertNotification(
                alertId = alertId,
                channelKey = channelKey,
                title = title,
                body = data["body"],
                source = data["source"],
                sourceKey = data["source_key"],
            )
        }
    }
}
