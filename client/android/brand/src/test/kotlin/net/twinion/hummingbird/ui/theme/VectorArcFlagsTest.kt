package net.twinion.hummingbird.ui.theme

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Test

/** Every `ic_*.xml` must inflate as a native `VectorDrawable`, not only as a
 * Compose `ImageVector`. The two parse SVG path data differently in one
 * place: SVG lets an arc's two flags be written compactly against the next
 * number (`A2 2 0 0112.67 19`, Lucide's own output), Compose's parser
 * accepts that, and Android's `PathParser` does not — "A needs to be
 * followed by a multiple of 7 floats" — so the phone's Compose screens drew
 * every glyph while the watch's tile, which goes through
 * `Resources.getDrawable`, could not load one (emulator pass, 2026-09-10).
 * This pins the flags as separate tokens, which both parsers read. */
class VectorArcFlagsTest {

    /** An arc command's first three numbers, then the second flag glued to
     * a digit or a point — the compact form. (A sign is a separator to both
     * parsers, so `0 1-2 2` is fine and not matched.) */
    private val compactFlags = Regex("""[Aa]\s*[-\d.]+[\s,]*[-\d.]+[\s,]*[-\d.]+[\s,]*[01][\s,]*[01](?=[\d.])""")

    @Test
    fun `no brand vector writes an arc's flags against its coordinates`() {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see client/android/build.gradle.kts)")
        val dir = File(root, "client/android/brand/src/main/res/drawable")
        val offenders = dir.listFiles { f -> f.name.startsWith("ic_") && f.extension == "xml" }!!
            .filter { file ->
                Regex("""android:pathData="([^"]*)"""").findAll(file.readText())
                    .any { compactFlags.containsMatchIn(it.groupValues[1]) }
            }
            .map { it.name }
        assertEquals("these vectors will not inflate natively — separate the arc flags", emptyList<String>(), offenders)
    }
}
