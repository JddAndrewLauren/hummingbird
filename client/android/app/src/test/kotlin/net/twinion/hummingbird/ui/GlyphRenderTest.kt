package net.twinion.hummingbird.ui

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The glyphs' draw-code pins (#558).
 *
 * A pixel-capture test was attempted first — compose one glyph per
 * position, `captureToImage()`, compare the outer ring's alpha — and
 * `captureToImage` times out under Robolectric here (`Condition still not
 * satisfied after 2000 ms`, the awaiting-draw wait; `@GraphicsMode(NATIVE)`
 * does not rescue it). So this is the fallback the slice plan named: pin
 * the draw code structurally — the three elements per glyph, the three
 * opacity stops, and that every element's alpha goes through the one
 * `stop()` function — and say so here, so the next reader knows a capture
 * was tried and why it is absent. If Roborazzi-class rendering lands
 * (docs/SURFACES.md's open question), replace this file with a real
 * capture.
 */
class GlyphRenderTest {

    private val src: String by lazy {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, "client/android/app/src/main/kotlin/net/twinion/hummingbird/ui/LevelGlyphs.kt")
        check(file.isFile) { "LevelGlyphs.kt not found under $root" }
        file.readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
    }

    @Test
    fun `the three opacity stops are the web's, verbatim`() {
        // custom-glyphs.tsx: UNEARNED 0.25, EARNED 1, UNSET 0.45.
        assertTrue(src.contains("UNEARNED = 0.25f"))
        assertTrue(src.contains("EARNED = 1f"))
        assertTrue(src.contains("UNSET = 0.45f"))
    }

    @Test
    fun `size draws three circles and energy three bars, every alpha through stop()`() {
        val circles = Regex("""drawCircle\(""").findAll(src).count()
        assertEquals("SizeGlyph is a dot and two rings", 3, circles)
        val bars = Regex("""drawBar\(""").findAll(src).count() - 1 // minus the definition
        assertEquals("EnergyGlyph is three bars", 3, bars)
        val stops = Regex("""stop\(\d, position\)""").findAll(src).count()
        assertEquals(
            "every element's fill must come from stop(element, position) — " +
                "a hardcoded alpha is the silent no-op this test exists to catch",
            6,
            stops,
        )
    }

    @Test
    fun `the geometry is the web viewBox, unrescaled`() {
        // The load-bearing numbers from custom-glyphs.tsx; a re-derived
        // geometry would drift from the web glyph beside which these render
        // on mixed surfaces. Rings r=3/6.75/10.5 stroke 2.5; bars at
        // x=4/9.75/15.5, y=14/9/4, heights 6/11/16.
        for (number in listOf("3f * s", "6.75f * s", "10.5f * s", "RING_STROKE = 2.5f",
            "4f * s, 14f * s", "9.75f * s, 9f * s", "15.5f * s, 4f * s")) {
            assertTrue("expected the web geometry literal `$number`", src.contains(number))
        }
    }

    @Test
    fun `the stop function maps unset, earned and unearned exactly`() {
        // The one place fill is decided: position 0 washes every element to
        // UNSET; otherwise element <= position is EARNED, the rest UNEARNED.
        assertTrue(
            Regex(
                """fun stop\(element: Int, position: Int\): Float = when \{\s*""" +
                    """position == 0 -> UNSET\s*""" +
                    """element <= position -> EARNED\s*""" +
                    """else -> UNEARNED\s*\}""",
            ).containsMatchIn(src),
        )
    }
}
