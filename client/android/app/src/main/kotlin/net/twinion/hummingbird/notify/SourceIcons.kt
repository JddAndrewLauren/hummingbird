package net.twinion.hummingbird.notify

import net.twinion.hummingbird.brand.R

// The icon a source draws beside its name (#922, ADR-0042).
//
// The slug is the **server's** word, from the source registry's `icon`
// (`server/domain/src/sources.rs`); this file is the device-side half of that
// pairing, and `SourceIconsTest` reads the registry to assert every slug the
// server can send has a drawable here. The reverse is the point of the
// fallback: a slug this build does not know (a registry newer than the app,
// an unregistered source, a payload from a server older than the field) draws
// the bell — the same glyph every alert wore before sources had their own —
// rather than nothing or a crash.
//
// Every drawable is white and outline-only: a notification small icon is read
// from its alpha channel alone and tinted by the system, and the list screens
// tint the same glyph with the label colour.
object SourceIcons {

    /** The slug the server sends for a source it does not know, and the
     * answer for any slug this build has no drawable for. */
    const val FALLBACK_SLUG = "bell"

    /** Every slug the registry may carry, and the drawable it draws. */
    val DRAWABLES: Map<String, Int> = mapOf(
        "auk" to R.drawable.ic_factory,
        "perch" to R.drawable.ic_bird,
        "starling" to R.drawable.ic_radio,
        "mail" to R.drawable.ic_mail,
        "calendar" to R.drawable.ic_calendar,
        "trash" to R.drawable.ic_trash,
        "flag" to R.drawable.ic_flag,
        "football" to R.drawable.ic_trophy,
        "pulse" to R.drawable.ic_activity,
        "github" to R.drawable.ic_git_branch,
        "task" to R.drawable.ic_circle_check,
        "home" to R.drawable.ic_home,
        "camera" to R.drawable.ic_camera,
    )

    /** The slug this build will actually draw for [slug]: itself when known,
     * [FALLBACK_SLUG] otherwise (including null, a payload with no icon). */
    fun resolve(slug: String?): String =
        if (slug != null && slug in DRAWABLES) slug else FALLBACK_SLUG

    /** The drawable for [slug], the bell's for an unknown one. */
    fun drawableFor(slug: String?): Int = slug?.let { DRAWABLES[it] } ?: R.drawable.ic_alert
}
