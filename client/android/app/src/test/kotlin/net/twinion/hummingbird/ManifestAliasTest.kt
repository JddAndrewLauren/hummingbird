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
// shortcut wiring the same screen needs. Parses the real manifest, the same
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

    @Test
    fun `the second launcher icon is an activity-alias targeting CaptureActivity`() {
        val application = manifest().children("application").single()
        val alias = application.children("activity-alias").singleOrNull()
            ?: error("no activity-alias found under <application>")

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

    @Test
    fun `CaptureActivity is declared exactly once and never carries its own LAUNCHER filter`() {
        val application = manifest().children("application").single()
        val activities = application.children("activity")
        val captureActivities = activities.filter { it.attr("name") == ".CaptureActivity" }

        assertEquals("CaptureActivity must be declared exactly once", 1, captureActivities.size)
        val captureActivity = captureActivities.single()

        // Only the alias may carry a LAUNCHER intent-filter — one entry
        // point per icon.
        val categories = captureActivity.children("intent-filter")
            .flatMap { it.children("category") }
            .mapNotNull { it.attr("name") }
        assertTrue(
            "CaptureActivity itself must not declare a LAUNCHER category",
            "android.intent.category.LAUNCHER" !in categories,
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
