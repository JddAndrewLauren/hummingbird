package net.twinion.hummingbird.ui.theme

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

// The type-token drift gate (#528, ADR-0026's companion to
// ColorTokenDriftTest): Font.kt's family-name constants must equal the
// first quoted family in each of tokens/fonts.css's --font-display,
// --font-sans and --font-mono declarations; the weight set each of
// SpaceGroteskFamily/SpaceMonoFamily bundles must equal the `wght@...` axis
// the same CSS's `@import` line requests for that family (Figtree's own
// axis there is a continuous `300..900` range, not an enumerable list, so
// it can't be pinned the same mechanical way — see the weight test's own
// comment); and every font resource each FontFamily in Font.kt references
// must exist under res/font/, with no extra, unreferenced file left in
// that directory. Same discipline as the colour gate: mechanical
// regex/text parsing over the real files, a gate rather than a compiler,
// so a mirror re-pull or a hand-edit that drops a weight fails loudly
// instead of shipping a silently wrong family.
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
    fun `Space Grotesk and Space Mono bundle exactly the weight axis the @import line requests`() {
        val cssWeights = parseImportWeightAxes()
        val kotlinWeights = parseFontKtWeightsByFamily()

        assertEquals(
            "Space Grotesk",
            cssWeights.getValue("Space+Grotesk").map(::weightName).toSet(),
            kotlinWeights["SpaceGroteskFamily"],
        )
        assertEquals(
            "Space Mono",
            cssWeights.getValue("Space+Mono").map(::weightName).toSet(),
            kotlinWeights["SpaceMonoFamily"],
        )

        // Figtree is deliberately narrower than its CSS axis: the token's
        // `@import` requests a continuous `300..900` range for the web's
        // arbitrary future use, which has no enumerable weight list to pin
        // against. This only pins that FigtreeFamily is non-empty and that
        // every weight it bundles is one Compose actually names — real
        // drift protection (Font.kt/res/font self-consistency) comes from
        // the resource-inventory test above, not from this CSS.
        val figtreeWeights = kotlinWeights["FigtreeFamily"].orEmpty()
        assertTrue("FigtreeFamily has no weights", figtreeWeights.isNotEmpty())
        assertTrue(
            "FigtreeFamily names a weight Compose doesn't have: $figtreeWeights",
            figtreeWeights.all { it in KNOWN_FONT_WEIGHT_NAMES },
        )
    }

    // -- fonts.css parsing --------------------------------------------------

    private fun parseFontsCss(): Map<String, String> {
        val css = repoFile(".claude/skills/hummingbird-design/tokens/fonts.css").readText()
        val decl = Regex("""(--font-[a-z]+)\s*:\s*"([^"]+)"""")
        val result = decl.findAll(css).associate { it.groupValues[1] to it.groupValues[2] }
        check(result.size >= 3) { "only ${result.size} --font-* declarations parsed from fonts.css — parser or file drifted" }
        return result
    }

    // Parses the `@import`'s Google Fonts CSS2 query string for each
    // family's `wght@` axis, e.g. `family=Space+Grotesk:wght@400;500;600;700`
    // → "Space+Grotesk" to [400, 500, 600, 700]. Families whose axis is a
    // range (`ital,wght@0,300..900;...`) yield an empty list rather than
    // failing the parse — Figtree's own test above knows not to compare
    // that to anything.
    private fun parseImportWeightAxes(): Map<String, List<Int>> {
        val css = repoFile(".claude/skills/hummingbird-design/tokens/fonts.css").readText()
        val importLine = Regex("""@import\s+url\("([^"]+)"\)""").find(css)
            ?: error("no @import url(...) line found in fonts.css — parser or file drifted")
        val query = importLine.groupValues[1]
        val family = Regex("""family=([\w+]+):wght@([0-9;]+)(?:&|$)""")
        val result = family.findAll(query).associate { m ->
            m.groupValues[1] to m.groupValues[2].split(";").map { it.toInt() }
        }
        check(result.isNotEmpty()) { "no family=...:wght@... axis parsed from the @import query — parser or URL shape drifted" }
        return result
    }

    private fun weightName(weight: Int): String = when (weight) {
        400 -> "Normal"
        500 -> "Medium"
        600 -> "SemiBold"
        700 -> "Bold"
        else -> error("unmapped CSS font-weight $weight — add it to weightName()")
    }

    private val KNOWN_FONT_WEIGHT_NAMES = setOf("Normal", "Medium", "SemiBold", "Bold")

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
