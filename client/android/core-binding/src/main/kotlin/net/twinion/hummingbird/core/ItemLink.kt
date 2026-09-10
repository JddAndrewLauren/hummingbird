package net.twinion.hummingbird.core

import android.content.Intent
import android.net.Uri

/** The one URI that names an item across the two devices: the watch fires it
 * through `RemoteActivityHelper` from an expanded row's "Open on phone", and
 * the phone's `.ItemLink` alias over `MainActivity` claims it (the Wear
 * capture design handoff, 2026-09-10). Defined once, here, because both
 * APKs build from this module and a scheme spelled twice is a scheme that
 * drifts.
 *
 * `hummingbird://item/<id>`, `ACTION_VIEW`, `CATEGORY_BROWSABLE` — the three
 * things `RemoteActivityHelper` requires of an intent it will carry. The id
 * is the authority's uuid, as every route already takes it; nothing here
 * interprets it. The notification tap's own `hummingbird://alert/<id>` is a
 * different host on the same scheme and is *not* a door (its extras decide,
 * `AlertNotifier`'s doc): [itemId] answers `null` to it. */
object ItemLink {
    const val SCHEME = "hummingbird"
    const val HOST = "item"

    fun uri(itemId: String): Uri = Uri.parse("$SCHEME://$HOST/$itemId")

    /** The intent the watch hands to the phone. */
    fun intent(itemId: String): Intent =
        Intent(Intent.ACTION_VIEW, uri(itemId)).addCategory(Intent.CATEGORY_BROWSABLE)

    /** The item an arriving intent names, or `null` when it is not an item
     * link — a launcher start, a notification tap, another host. */
    fun itemId(intent: Intent?): String? = itemId(intent?.action, intent?.dataString)

    /** The same, over the two strings — the testable half: `android.net.Uri`
     * is a stub on the JVM, and the grammar is one prefix. */
    internal fun itemId(action: String?, data: String?): String? {
        if (action != Intent.ACTION_VIEW || data == null || !data.startsWith(PREFIX)) return null
        val id = data.removePrefix(PREFIX)
        return id.takeIf { it.isNotEmpty() && !it.contains('/') && !it.contains('?') }
    }

    private const val PREFIX = "$SCHEME://$HOST/"
}
