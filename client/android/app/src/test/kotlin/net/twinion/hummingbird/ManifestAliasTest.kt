package net.twinion.hummingbird

import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.w3c.dom.Element

private const val ANDROID_NS = "http://schemas.android.com/apk/res/android"

// The manifest-alias gate (#128/#503's CI acceptance): a parsed-XML check
// that the second launcher icon exists in the shape the 2026-08-14 grilling
// settled — an activity-alias targeting CaptureActivity, its own icon and
// label, and a MAIN/LAUNCHER intent-filter — plus the permission and
// shortcut wiring the same screen needs. Since #782 a second alias, the
// share-sheet door, sits beside it, so every alias is selected by NAME:
// `singleOrNull()` over the aliases was the trap that broke the moment the
// second one existed. Parses the real manifest, the same
// "the file is the authority, not a hand-copied expectation" discipline
// `ColorTokenDriftTest` already uses for its own no-emulator gate.
class ManifestAliasTest {

    private fun manifest(): Element {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, "client/android/app/src/main/AndroidManifest.xml")
        check(file.isFile) { "AndroidManifest.xml not found under $root" }
        val factory = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
        return factory.newDocumentBuilder().parse(file).documentElement
    }

    private fun Element.attr(name: String): String? =
        getAttributeNS(ANDROID_NS, name).ifEmpty { null }

    private fun Element.children(tag: String): List<Element> =
        (0 until childNodes.length)
            .mapNotNull { childNodes.item(it) as? Element }
            .filter { it.tagName == tag }

    private fun alias(name: String): Element =
        manifest().children("application").single()
            .children("activity-alias")
            .singleOrNull { it.attr("name") == name }
            ?: error("no activity-alias named $name under <application>")

    @Test
    fun `the second launcher icon is an activity-alias targeting CaptureActivity`() {
        val alias = alias(".CaptureLauncher")

        assertEquals(".CaptureActivity", alias.attr("targetActivity"))
        assertEquals("true", alias.attr("exported"))
        assertTrue("alias has no icon", !alias.attr("icon").isNullOrEmpty())
        assertTrue(
            "the second icon must differ from the main launcher icon",
            alias.attr("icon") != "@mipmap/ic_launcher",
        )
        assertTrue("alias has no label", !alias.attr("label").isNullOrEmpty())

        val filters = alias.children("intent-filter")
        assertTrue("activity-alias has no intent-filter", filters.isNotEmpty())
        val actions = filters.flatMap { it.children("action") }.mapNotNull { it.attr("name") }
        val categories = filters.flatMap { it.children("category") }.mapNotNull { it.attr("name") }
        assertTrue("no MAIN action", actions.contains("android.intent.action.MAIN"))
        assertTrue("no LAUNCHER category", categories.contains("android.intent.category.LAUNCHER"))
    }

    /** #782's share-sheet door: the app's own name and the capture icon
     * over the same CaptureActivity, filtering a `text/plain` SEND and
     * nothing wider. Exactly these three lines, because each widening is
     * a decision the issue took the other way — images, files and
     * SEND_MULTIPLE are out of scope. */
    @Test
    fun `the share target is a second alias over CaptureActivity, text-plain SEND only`() {
        val alias = alias(".ShareTarget")

        assertEquals(".CaptureActivity", alias.attr("targetActivity"))
        assertEquals("true", alias.attr("exported"))
        assertEquals("@mipmap/ic_launcher_capture", alias.attr("icon"))
        assertEquals("@string/share_target_label", alias.attr("label"))

        val filter = alias.children("intent-filter").singleOrNull()
            ?: error("the share target must carry exactly one intent-filter")
        assertEquals(
            listOf("android.intent.action.SEND"),
            filter.children("action").mapNotNull { it.attr("name") },
        )
        assertEquals(
            listOf("android.intent.category.DEFAULT"),
            filter.children("category").mapNotNull { it.attr("name") },
        )
        assertEquals(
            listOf("text/plain"),
            filter.children("data").mapNotNull { it.attr("mimeType") },
        )

        // The label's value, since the sheet shows it: the product name,
        // lowercase everywhere.
        val root = System.getProperty("hummingbird.repoRoot")!!
        val strings = File(root, "client/android/app/src/main/res/values/strings.xml").readText()
        assertTrue(
            "share_target_label must be the lowercase product name",
            strings.contains("<string name=\"share_target_label\">hummingbird</string>"),
        )
    }

    /** The share seeds through the core's mapping (`parseSharePayload`,
     * ADR-0025) and nothing of Kotlin's own: no regex, no scanning the
     * text for `http`. */
    @Test
    fun `the share payload is parsed by the seam, never by Kotlin`() {
        val root = System.getProperty("hummingbird.repoRoot")!!
        val src = File(
            root,
            "client/android/app/src/main/kotlin/net/twinion/hummingbird/CaptureActivity.kt",
        ).readText()
        assertTrue("CaptureActivity must seed through parseSharePayload(", src.contains("parseSharePayload("))
        assertTrue("and read the SEND action", src.contains("Intent.ACTION_SEND"))
        assertTrue(
            "CaptureActivity must not parse the payload itself",
            !src.contains("Regex(") && !src.contains("indexOf(\"http"),
        )
    }

    @Test
    fun `CaptureActivity is declared exactly once and never carries its own LAUNCHER filter`() {
        val application = manifest().children("application").single()
        val activities = application.children("activity")
        val captureActivities = activities.filter { it.attr("name") == ".CaptureActivity" }

        assertEquals("CaptureActivity must be declared exactly once", 1, captureActivities.size)
        val captureActivity = captureActivities.single()

        // Only an alias may carry an intent-filter — one entry point per
        // door, each alias naming itself.
        assertTrue(
            "CaptureActivity itself must carry no intent-filter at all",
            captureActivity.children("intent-filter").isEmpty(),
        )
    }

    @Test
    fun `RECORD_AUDIO is requested for the mic button`() {
        val permissions = manifest().children("uses-permission").mapNotNull { it.attr("name") }
        assertTrue("RECORD_AUDIO permission missing", permissions.contains("android.permission.RECORD_AUDIO"))
    }

    @Test
    fun `POST_NOTIFICATIONS is requested for the alert lane`() {
        // minSdk 35 — this is always a runtime grant, but the manifest
        // declaration is what makes the request legal at all; without it
        // `requestPermissions` returns an immediate denial.
        val permissions = manifest().children("uses-permission").mapNotNull { it.attr("name") }
        assertTrue(
            "POST_NOTIFICATIONS permission missing",
            permissions.contains("android.permission.POST_NOTIFICATIONS"),
        )
    }

    @Test
    fun `ACCESS_NOTIFICATION_POLICY is declared, or urgent can never bypass DND`() {
        // Declaring it grants nothing — the grant is the user's, in
        // Settings > Do Not Disturb access — but an app that does not
        // declare it never appears in that list, so
        // `NotificationChannel.setBypassDnd(true)` stays inert for the
        // life of the install and ADR-0012's "must not get caught in
        // blanket silencing" quietly fails.
        val permissions = manifest().children("uses-permission").mapNotNull { it.attr("name") }
        assertTrue(
            "ACCESS_NOTIFICATION_POLICY permission missing",
            permissions.contains("android.permission.ACCESS_NOTIFICATION_POLICY"),
        )
    }

    @Test
    fun `the FCM service is declared exactly once, unexported, on the MESSAGING_EVENT action`() {
        val application = manifest().children("application").single()
        val services = application.children("service")
            .filter { it.attr("name") == ".push.HbMessagingService" }
        assertEquals("HbMessagingService must be declared exactly once", 1, services.size)
        val service = services.single()

        // Play services binds it through the action, not through export —
        // an exported service here would be a needless attack surface.
        assertEquals("false", service.attr("exported"))
        val actions = service.children("intent-filter")
            .flatMap { it.children("action") }
            .mapNotNull { it.attr("name") }
        assertTrue(
            "the FCM service must filter on com.google.firebase.MESSAGING_EVENT",
            actions.contains("com.google.firebase.MESSAGING_EVENT"),
        )
    }

    @Test
    fun `the Ack receiver is declared exactly once, unexported, on its own action`() {
        val application = manifest().children("application").single()
        val receivers = application.children("receiver")
            .filter { it.attr("name") == ".push.AckReceiver" }
        assertEquals("AckReceiver must be declared exactly once", 1, receivers.size)
        val receiver = receivers.single()

        // Exported, any app on the device could settle alerts on the
        // authority; the only legitimate sender is this app's own
        // PendingIntent.
        assertEquals("false", receiver.attr("exported"))
        val actions = receiver.children("intent-filter")
            .flatMap { it.children("action") }
            .mapNotNull { it.attr("name") }
        assertTrue(
            "the Ack receiver must filter on net.twinion.hummingbird.action.ACK_ALERT",
            actions.contains("net.twinion.hummingbird.action.ACK_ALERT"),
        )
    }

    @Test
    fun `no shipped icon resource still carries the placeholder comment`() {
        // #528: the real brand icon replaced the M0/M1 monochrome glyph
        // marks, each of which said outright it was a placeholder pending
        // the mirrored artwork. This is the mechanical guard against a
        // future edit reintroducing one of those files (or a new one)
        // without also carrying the real art -- text resources under
        // res/drawable*/ and res/mipmap*/ are read as text (icon PNGs
        // themselves can't carry a comment, so only their XML wrappers and
        // any vector art are in scope) and none may mention "placeholder".
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val resDir = File(root, "client/android/app/src/main/res")
        val iconDirs = resDir.listFiles { f -> f.isDirectory && (f.name.startsWith("drawable") || f.name.startsWith("mipmap")) }
            ?: error("res/ not found or unreadable under $resDir")
        assertTrue("no drawable*/mipmap* resource directories found", iconDirs.isNotEmpty())

        val offenders = mutableListOf<String>()
        for (dir in iconDirs) {
            for (file in dir.listFiles { f -> f.isFile && (f.extension == "xml") } ?: emptyArray()) {
                if (file.readText().contains("placeholder", ignoreCase = true)) {
                    offenders += "${dir.name}/${file.name}"
                }
            }
        }
        assertTrue(
            "these shipped icon resources still carry a placeholder comment: $offenders",
            offenders.isEmpty(),
        )
    }

    @Test
    fun `static shortcuts are wired on the primary launcher activity, and the resource exists`() {
        val application = manifest().children("application").single()
        val mainActivity = application.children("activity").single { it.attr("name") == ".MainActivity" }
        val shortcutsMeta = mainActivity.children("meta-data")
            .singleOrNull { it.attr("name") == "android.app.shortcuts" }
            ?: error("no android.app.shortcuts meta-data on MainActivity")
        assertEquals("@xml/shortcuts", shortcutsMeta.attr("resource"))

        val root = System.getProperty("hummingbird.repoRoot")!!
        val shortcutsFile = File(root, "client/android/app/src/main/res/xml/shortcuts.xml")
        assertTrue("shortcuts.xml missing", shortcutsFile.isFile)
    }
}
