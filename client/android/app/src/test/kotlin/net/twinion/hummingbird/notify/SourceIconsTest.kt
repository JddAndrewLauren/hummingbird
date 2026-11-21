package net.twinion.hummingbird.notify

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// The icon half of ADR-0042's pairing: the registry names a slug
// (`server/domain/src/sources.rs`), the phone draws it, and a slug it does not
// know draws the bell.
class SourceIconsTest {

    @Test
    fun `a known slug resolves to itself`() {
        assertEquals("auk", SourceIcons.resolve("auk"))
        assertEquals("perch", SourceIcons.resolve("perch"))
    }

    @Test
    fun `an unknown, missing or blank slug falls back to the bell`() {
        assertEquals("bell", SourceIcons.resolve("a-slug-from-a-newer-registry"))
        assertEquals("bell", SourceIcons.resolve(null))
        assertEquals("bell", SourceIcons.resolve(""))
        // `bell` itself is the fallback's name, not a drawable key: the server
        // sends it for an unregistered source and it lands on the bell too.
        assertEquals("bell", SourceIcons.resolve("bell"))
    }

    @Test
    fun `an unknown slug draws the same drawable the bell always was`() {
        assertEquals(SourceIcons.drawableFor(null), SourceIcons.drawableFor("not-a-slug"))
        assertTrue(SourceIcons.drawableFor("auk") != SourceIcons.drawableFor("not-a-slug"))
    }

    @Test
    fun `every icon slug the registry can send has a drawable here`() {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see client/android/build.gradle.kts)")
        val registry = File(root, "server/domain/src/sources.rs")
        assertTrue("sources.rs not found under $root", registry.isFile)
        // Only the REGISTRY array carries `icon: "…"` lines.
        val slugs = Regex("""^\s*icon:\s*"([^"]+)",""", RegexOption.MULTILINE)
            .findAll(registry.readText()).map { it.groupValues[1] }.toSet()
        assertTrue("found no icon slugs in the registry", slugs.size >= 10)
        val missing = slugs.filter { it !in SourceIcons.DRAWABLES && it != SourceIcons.FALLBACK_SLUG }
        assertEquals("registry slugs with no drawable (they would draw the bell)", emptyList<String>(), missing)
    }
}
