package net.twinion.hummingbird.wear

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// ADR-0039's colour rule for the watch, mechanically: every colour the
// watch draws is a `:brand` constant, so the design mirror's drift gate
// (`ColorTokenDriftTest`, over `Color.kt`) covers the watch for free. A
// `Color(0x…)` literal under `wear/src/main` would be a value that gate
// cannot see — the exact silent failure ADR-0026 accepted hand-porting to
// guard against — so none may exist. Source-text pin, like the phone's own
// structural gates.
class WearThemeStructuralTest {

    @Test
    fun `no Color literal anywhere under wear src main`() {
        val offenders = wearMain()
            .walkTopDown()
            .filter { it.isFile && it.extension == "kt" }
            // Comments stripped first: the rule may be *stated* in a header.
            .filter { Regex("""Color\(\s*0x""").containsMatchIn(withoutComments(it.readText())) }
            .map { it.relativeTo(wearMain()).path }
            .toList()
        assertEquals(emptyList<String>(), offenders)
    }

    @Test
    fun `the theme is built from brand constants alone`() {
        val theme = File(wearMain(), "kotlin/net/twinion/hummingbird/wear/ui/theme/WearTheme.kt").readText()
        assertTrue(
            "WearTheme must import its colours from the brand's Color.kt",
            theme.contains("import net.twinion.hummingbird.ui.theme."),
        )
        val type = File(wearMain(), "kotlin/net/twinion/hummingbird/wear/ui/theme/WearType.kt").readText()
        for (family in listOf("SpaceGroteskFamily", "FigtreeFamily", "SpaceMonoFamily")) {
            assertTrue("WearType must reach the brand's $family", type.contains(family))
        }
    }

    @Test
    fun `the launcher plate is the phone's, byte for byte`() {
        // The phone's copy is the one `ColorTokenDriftTest` gates against
        // the design mirror; the watch's copy must equal it or drift twice.
        val hex = Regex("""name="ic_launcher_background">(#[0-9a-fA-F]+)<""")
        val phone = hex.find(File(repoRoot(), "client/android/app/src/main/res/values/colors.xml").readText())
        val watch = hex.find(File(wearMain(), "res/values/colors.xml").readText())
        assertEquals(phone?.groupValues?.get(1), watch?.groupValues?.get(1))
        assertTrue("both files must declare ic_launcher_background", phone != null && watch != null)
    }

    private fun withoutComments(src: String): String =
        src.replace(Regex("""/\*[\s\S]*?\*/"""), "").replace(Regex("""(?m)^\s*//.*$"""), "")

    private fun repoRoot(): File =
        File(
            System.getProperty("hummingbird.repoRoot")
                ?: error("hummingbird.repoRoot not set — run under Gradle (see client/android/build.gradle.kts)"),
        )

    private fun wearMain(): File {
        val dir = File(repoRoot(), "client/android/wear/src/main")
        check(dir.isDirectory) { "wear/src/main not found under ${repoRoot()}" }
        return dir
    }
}
