package net.twinion.hummingbird.ui.theme

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Test

// The motion-token drift gate (#801, ADR-0026's third): Motion.kt's DurBase
// and EaseFlit must equal `--dur-base` and `--ease-flit` in the design
// mirror's tokens/motion.css. Same discipline as the colour and type gates
// — mechanical text parsing over the real file, the CSS is the authority
// and the Kotlin is what may be wrong — and here for the same silent
// failure: a mirror re-pull changes a duration, web picks it up by copy,
// and Android keeps animating at the stale one with nothing visibly broken.
//
// The `:root` scope alone is read, deliberately. `motion.css`'s
// `prefers-reduced-motion` block collapses every duration to 1ms, and
// Android answers that switch a different way — `reducedMotion()` reads the
// platform's own animator scale and callers branch on it — so pinning the
// collapsed values here would pin a mechanism this client does not use.
//
// The carry spring is NOT pinned, and cannot be: `carrySpring()` is a
// damping ratio and a stiffness, and the mirror has no token for it. That
// is stated in `motion.css`'s own absence rather than worked around — the
// spring is not a duration, and ADR-0021 decision 9 is where its numbers
// live.
class MotionTokenDriftTest {

    @Test
    fun `DurBase equals --dur-base in the design mirror`() {
        assertEquals("200ms", rootTokens()["--dur-base"])
        assertEquals(200, DurBase)
    }

    @Test
    fun `EaseFlit equals --ease-flit in the design mirror`() {
        assertEquals("cubic-bezier(.2,.8,.2,1)", rootTokens()["--ease-flit"])
        // Compose's CubicBezierEasing carries no readable control points, so
        // the pin is the constructor call in the source — the same text-over
        // -source approach the colour gate takes to Color.kt.
        val declaration = motionKt()
            .lineSequence()
            .first { it.contains("val EaseFlit") }
        assertEquals(
            "val EaseFlit: CubicBezierEasing = CubicBezierEasing(0.2f, 0.8f, 0.2f, 1f)",
            declaration.trim(),
        )
    }

    @Test
    fun `the reduced-motion read lives here and nowhere else`() {
        // It was `StatusQuietStack.expandSpec`'s alone until this slice, and
        // two animator-scale reads would be two places to keep honest. Every
        // module's production Kotlin is walked (`:app`, `:core-binding`, and
        // whatever ADR-0039 adds next), not `:app`'s alone — a module split
        // must not open a second place for the read.
        val offenders = repoFile("client/android")
            .walkTopDown()
            .filter { it.isFile && it.extension == "kt" }
            .filter { "/src/main/kotlin/" in it.path }
            .filter { it.name != "Motion.kt" }
            .filter { it.readText().contains("ANIMATOR_DURATION_SCALE") }
            .map { it.name }
            .toList()
        assertEquals(emptyList<String>(), offenders)
    }

    // -- Parsing ----------------------------------------------------------

    /** `:root`'s own declarations, stopping at the media query below them. */
    private fun rootTokens(): Map<String, String> {
        val css = repoFile(".claude/skills/hummingbird-design/tokens/motion.css").readText()
        val root = css.substringAfter(":root{").substringBefore("@media")
        return Regex("""(--[a-z-]+)\s*:\s*([^;]+);""")
            .findAll(root)
            .associate { it.groupValues[1] to it.groupValues[2].trim() }
    }

    private fun motionKt(): String =
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/ui/theme/Motion.kt")
            .readText()

    private fun repoFile(relative: String): File {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see client/android/build.gradle.kts)")
        val f = File(root, relative)
        check(f.isFile || f.isDirectory) { "$relative not found under $root" }
        return f
    }
}
