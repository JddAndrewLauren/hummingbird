package net.twinion.hummingbird.ui.theme

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

// The type-token drift gate (#528, ADR-0026's companion to
// ColorTokenDriftTest): Font.kt's family-name constants must equal the
// first quoted family in each of tokens/fonts.css's --font-display,
// --font-sans and --font-mono declarations, and every font resource each
// FontFamily in Font.kt references must exist under res/font/ — with no
// extra, unreferenced file left in that directory. Same discipline as the
// colour gate: mechanical regex/text parsing over the real files, a gate
// rather than a compiler, so a mirror re-pull or a hand-edit that drops a
// weight fails loudly instead of shipping a silently wrong family.
class TypeTokenDriftTest {

    @Test
    fun `Font family-name constants equal tokens fonts css`() {
        val css = parseFontsCss()
        assertEquals(css["--font-display"], SPACE_GROTESK_FAMILY_NAME)
        assertEquals(css["--font-sans"], FIGTREE_FAMILY_NAME)
        assertEquals(css["--font-mono"], SPACE_MONO_FAMILY_NAME)
    }

    @Test
    fun `every font resource Font-kt references exists under res-font`() {
        val referenced = parseFontKtResourceNames()
        assertTrue("Font.kt referenced no font resources — parser or file drifted", referenced.isNotEmpty())

        val dir = repoFile("client/android/app/src/main/res/font")
        val onDisk = dir.listFiles { f -> f.isFile }
            ?.map { it.nameWithoutExtension }
            ?.toSet()
            ?: error("res/font not found or unreadable")

        val missing = referenced - onDisk
        val unreferenced = onDisk - referenced
        if (missing.isNotEmpty() || unreferenced.isNotEmpty()) {
            fail(
                "Font.kt and res/font/ have drifted apart:\n" +
                    "  referenced but missing from res/font/: $missing\n" +
                    "  present in res/font/ but never referenced: $unreferenced",
            )
        }
    }

    @Test
    fun `the bundled weight inventory matches the brief -- Space Grotesk 400-700, Space Mono 400-700`() {
        val weights = parseFontKtWeightsByFamily()
        assertEquals(setOf("Normal", "Medium", "SemiBold", "Bold"), weights["SpaceGroteskFamily"])
        assertEquals(setOf("Normal", "Bold"), weights["SpaceMonoFamily"])
        // Figtree is deliberately narrower -- "the weights actually used" --
        // so this only pins that it's non-empty and a subset of the four
        // named weights above; TypographySourcesFigtree below pins the
        // "actually used" half of that claim.
        assertTrue("FigtreeFamily has no weights", weights["FigtreeFamily"]?.isNotEmpty() == true)
    }

    // -- fonts.css parsing --------------------------------------------------

    private fun parseFontsCss(): Map<String, String> {
        val css = repoFile(".claude/skills/hummingbird-design/tokens/fonts.css").readText()
        val decl = Regex("""(--font-[a-z]+)\s*:\s*"([^"]+)"""")
        val result = decl.findAll(css).associate { it.groupValues[1] to it.groupValues[2] }
        check(result.size >= 3) { "only ${result.size} --font-* declarations parsed from fonts.css — parser or file drifted" }
        return result
    }

    // -- Font.kt parsing ------------------------------------------------------

    private fun fontKtText(): String =
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/ui/theme/Font.kt").readText()

    private fun parseFontKtResourceNames(): Set<String> =
        Regex("""R\.font\.(\w+)""").findAll(fontKtText()).map { it.groupValues[1] }.toSet()

    private fun parseFontKtWeightsByFamily(): Map<String, Set<String>> {
        // Non-greedy up to the next line that is only a closing paren: each
        // nested Font(...) call closes on its own line ("...Bold),"), so
        // the family block's own close is the first "\n)" the lazy match
        // reaches.
        val familyBlock = Regex("""val (\w+Family) = FontFamily\((.*?)\n\)""", RegexOption.DOT_MATCHES_ALL)
        val weightIn = Regex("""FontWeight\.(\w+)""")
        return familyBlock.findAll(fontKtText()).associate { m ->
            m.groupValues[1] to weightIn.findAll(m.groupValues[2]).map { it.groupValues[1] }.toSet()
        }
    }

    private fun repoFile(relative: String): File {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val f = File(root, relative)
        check(f.isFile || f.isDirectory) { "$relative not found under $root" }
        return f
    }
}
