package net.twinion.hummingbird.wear

import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.w3c.dom.Element

// The watch manifest's load-bearing declarations, pinned (ADR-0039). Each
// is a line a green build does not check and a wrong one fails silently on
// the device: a missing watch feature installs on a phone; a missing
// `standalone` makes the Play/side-load flow demand a companion; a
// launcher-less MainActivity is an app with no door. The phone's
// `ManifestAliasTest` is the model — parse the real XML, assert on it.
class WearManifestStructuralTest {

    @Test
    fun `the app declares itself a watch app, standalone`() {
        val m = manifest()
        val features = m.children("uses-feature").map { it.attr("name") }
        assertTrue("uses-feature android.hardware.type.watch", "android.hardware.type.watch" in features)
        val application = m.children("application").single()
        val standalone = application.children("meta-data")
            .single { it.attr("name") == "com.google.android.wearable.standalone" }
        assertEquals("true", standalone.attr("value"))
        val wearableLib = application.children("uses-library")
            .single { it.attr("name") == "com.google.android.wearable" }
        assertEquals("true", wearableLib.attr("required"))
    }

    @Test
    fun `MainActivity is the exported launcher`() {
        val application = manifest().children("application").single()
        val main = application.children("activity").single { it.attr("name") == ".MainActivity" }
        assertEquals("true", main.attr("exported"))
        val filter = main.children("intent-filter").single()
        assertEquals("android.intent.action.MAIN", filter.children("action").single().attr("name"))
        assertEquals("android.intent.category.LAUNCHER", filter.children("category").single().attr("name"))
    }

    @Test
    fun `the watch asks for no microphone and no notification permission`() {
        // Capture dictates through the system input chooser (ADR-0022 D4);
        // there is no push lane on the watch this slice. Either permission
        // appearing would mean one of those decisions was reopened in code
        // without the ADR.
        val permissions = manifest().children("uses-permission").map { it.attr("name") }
        assertTrue("no RECORD_AUDIO", "android.permission.RECORD_AUDIO" !in permissions)
        assertTrue("no POST_NOTIFICATIONS", "android.permission.POST_NOTIFICATIONS" !in permissions)
    }

    @Test
    fun `CaptureActivity is exported, on its own task, and out of Recents`() {
        val application = manifest().children("application").single()
        val capture = application.children("activity").single { it.attr("name") == ".capture.CaptureActivity" }
        assertEquals("true", capture.attr("exported"))
        assertEquals("true", capture.attr("excludeFromRecents"))
        assertTrue("its own taskAffinity", capture.attr("taskAffinity").isNotEmpty())
        assertTrue("no launcher filter on the capture activity", capture.children("intent-filter").isEmpty())
    }

    @Test
    fun `the token listener is bound to the Data Layer on the hummingbird prefix`() {
        val application = manifest().children("application").single()
        val listener = application.children("service").single { it.attr("name") == ".token.TokenListenerService" }
        assertEquals("true", listener.attr("exported"))
        val filter = listener.children("intent-filter").single()
        assertEquals("com.google.android.gms.wearable.MESSAGE_RECEIVED", filter.children("action").single().attr("name"))
        val data = filter.children("data").single()
        assertEquals("wear", data.attr("scheme"))
        assertEquals("*", data.attr("host"))
        assertEquals("/hummingbird", data.attr("pathPrefix"))
    }

    // -- Parsing ----------------------------------------------------------

    private fun manifest(): Element {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see client/android/build.gradle.kts)")
        val file = File(root, "client/android/wear/src/main/AndroidManifest.xml")
        check(file.isFile) { "wear manifest not found under $root" }
        return DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(file).documentElement
    }

    private fun Element.children(tag: String): List<Element> {
        val out = mutableListOf<Element>()
        val nodes = childNodes
        for (i in 0 until nodes.length) {
            val n = nodes.item(i)
            if (n is Element && n.tagName == tag) out += n
        }
        return out
    }

    private fun Element.attr(name: String): String = getAttribute("android:$name")
}
