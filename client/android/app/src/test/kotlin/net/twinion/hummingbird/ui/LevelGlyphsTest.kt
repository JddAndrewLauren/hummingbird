package net.twinion.hummingbird.ui

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** #558's acceptance pin, both halves: the ramp is indexed by **position on
 * the scale, not by value name** (ADR-0024 — one table serving both
 * dimensions is what prevents the drift the ADR guards against).
 *
 * The behavioural half proves `levelPosition` reads only the list index;
 * the structural half proves the file *cannot* be indexed by value name —
 * none of the six vocabulary words appears in it, and `levelColor`'s
 * signature takes a position. A per-dimension colour map cannot creep in
 * without failing one of the two.
 */
class LevelGlyphsTest {

    @Test
    fun `position is the list index plus one, whatever the words are`() {
        // Deliberately not the real vocabulary: the function must never
        // inspect the word, only where it sits.
        val vocabulary = listOf("a", "b", "c")
        assertEquals(1, levelPosition(vocabulary, "a"))
        assertEquals(2, levelPosition(vocabulary, "b"))
        assertEquals(3, levelPosition(vocabulary, "c"))
    }

    @Test
    fun `null and an unknown word both land on position zero — unset`() {
        val vocabulary = listOf("a", "b", "c")
        assertEquals(0, levelPosition(vocabulary, null))
        assertEquals(0, levelPosition(vocabulary, "z"))
    }

    @Test
    fun `one table answers both dimensions — the same position gives the same colour`() {
        // quick (size position 1) and low (energy position 1) must resolve
        // to one answer; a second per-dimension map would let them drift.
        val sizeish = levelColor(levelPosition(listOf("s1", "s2", "s3"), "s1"), dark = false)
        val energyish = levelColor(levelPosition(listOf("e1", "e2", "e3"), "e1"), dark = false)
        assertEquals(sizeish, energyish)
    }

    @Test
    fun `the glyph-only accessible names speak sentence case and admit not judged`() {
        assertEquals("Size: quick", sizeTitle("quick"))
        assertEquals("Size: not judged", sizeTitle(null))
        assertEquals("Energy: high", energyTitle("high"))
        assertEquals("Energy: not judged", energyTitle(null))
    }

    // -- The structural half.

    private val src: String by lazy {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, "client/android/app/src/main/kotlin/net/twinion/hummingbird/ui/LevelGlyphs.kt")
        check(file.isFile) { "LevelGlyphs.kt not found under $root" }
        // Comments stripped so the header may name the vocabulary words it
        // forbids below (`RecallScreenStructuralTest`'s refinement).
        file.readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
    }

    @Test
    fun `no vocabulary word appears in the glyph module — it cannot be indexed by name`() {
        for (word in listOf("quick", "normal", "deep", "low", "medium", "high")) {
            assertFalse(
                "LevelGlyphs.kt must not know the word \"$word\" — position is the only key",
                src.contains("\"$word\""),
            )
        }
        assertTrue(
            "levelColor must take a position, not a value",
            src.contains("fun levelColor(position: Int"),
        )
    }

    @Test
    fun `exactly one four-entry ramp table exists per scheme`() {
        val tables = Regex("""listOf\((?:\s*[A-Za-z0-9]+,){3}\s*[A-Za-z0-9]+,?\s*\)""")
            .findAll(src)
            .count()
        assertEquals(
            "one RAMP_LIGHT and one RAMP_DARK — a second per-dimension map is the drift ADR-0024 forbids",
            2,
            tables,
        )
    }
}
