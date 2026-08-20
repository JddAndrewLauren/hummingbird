package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

// The app top bar (#614) — the design kit's Android `TopBar`: icon plate,
// lowercase wordmark, and the Recall trigger every web surface's header
// carries. It is chrome added to `MainActivity`, and `docs/SURFACES.md`'s
// Android rule is that a screen added without a structural test leaves this
// surface with no evidence at all — chrome on the always-composed root is
// no exception, so this pins that the bar exists, says the brand's name,
// and that its search glyph really reaches Recall.
class TopBarStructuralTest {

    private fun source(name: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, "client/android/app/src/main/kotlin/net/twinion/hummingbird/$name")
        check(file.isFile) { "$name not found under $root" }
        return file.readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
    }

    @Test
    fun `the Scaffold carries the top bar, and it is the branded AppTopBar`() {
        val src = source("MainActivity.kt")
        assertTrue(
            "MainActivity's Scaffold must fill its topBar slot",
            src.contains("topBar = {"),
        )
        assertTrue(
            "the slot's content is the AppTopBar composable",
            src.contains("AppTopBar("),
        )
        assertTrue(
            "the bar carries the lowercase wordmark",
            src.contains("\"hummingbird\""),
        )
    }

    @Test
    fun `the search trigger names itself the way every web trigger does, and opens Recall`() {
        val src = source("MainActivity.kt")
        assertTrue(
            "the trigger's accessible name is the shared web wording",
            src.contains("contentDescription = \"Search everything\""),
        )
        assertTrue(
            "the trigger must open the search overlay, never navigate (the web overlay's contract)",
            src.contains("onSearch = { recallOpen = true }"),
        )
    }

    @Test
    fun `the icon plate swaps with the resolved theme, never one export for both`() {
        val src = source("MainActivity.kt")
        assertTrue(
            "light and dark plates are two separate exports, swapped on the resolved theme",
            src.contains("if (dark) R.drawable.app_icon_dark else R.drawable.app_icon_light"),
        )
    }
}
