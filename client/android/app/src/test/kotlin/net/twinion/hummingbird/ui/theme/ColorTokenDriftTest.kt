package net.twinion.hummingbird.ui.theme

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.fail
import org.junit.Test

// The token drift gate (#483, ADR-0026): every constant in Color.kt must
// equal its token in the design mirror's tokens/colors.css. The CSS is the
// authority; the Kotlin is what may be wrong. ADR-0026 chose hand-porting
// over codegen, so this test is the guard against the silent failure that
// choice admits — a mirror re-pull changes a hex, web picks it up by copy,
// and Android keeps rendering the stale colour with nothing visibly broken.
//
// Both files are parsed with line/block regexes over source text. That is
// deliberate: this is a gate, not a compiler. A Color.kt constant with no
// mapping entry below FAILS — the mechanical form of that file's "never a
// value the CSS doesn't have".
class ColorTokenDriftTest {

    // -- The mapping ------------------------------------------------------

    // Ramp constants (Ember500 ↔ --ember-500) map mechanically; these are
    // the ones that don't. Each names its scope: the same token name
    // carries different values in `:root` (light) and `[data-theme="dark"]`.
    private val semanticOutliers = mapOf(
        "CrimsonDark" to ScopedToken("--status-danger-fg", Scope.DARK),
        "BorderDefaultLight" to ScopedToken("--border-default", Scope.LIGHT),
        "SurfaceSunkenDark" to ScopedToken("--surface-sunken", Scope.DARK),
        "StatusInfoFgDark" to ScopedToken("--status-info-fg", Scope.DARK),
        "BorderDefaultDark" to ScopedToken("--border-default", Scope.DARK),
        "BorderSubtleDark" to ScopedToken("--border-subtle", Scope.DARK),
        "StatusDangerBgDark" to ScopedToken("--status-danger-bg", Scope.DARK),
        // `--urgency-soon` diverges between scopes (dark is a literal, not
        // a ramp alias) — see `Color.kt`'s own comment on `UrgencySoonDark`.
        "UrgencySoonDark" to ScopedToken("--urgency-soon", Scope.DARK),
        // Same divergence for `--urgency-overdue` — see `Color.kt`'s own
        // comment on `UrgencyOverdueDark`.
        "UrgencyOverdueDark" to ScopedToken("--urgency-overdue", Scope.DARK),
    )

    // A ramp name is CamelCase-with-trailing-digits; anything else must be
    // in semanticOutliers or the constant is unmapped and the gate fails.
    private val rampName = Regex("""^([A-Z][a-z]+)(\d+)$""")

    private fun mappingFor(constant: String): ScopedToken? {
        semanticOutliers[constant]?.let { return it }
        val m = rampName.matchEntire(constant) ?: return null
        return ScopedToken("--${m.groupValues[1].lowercase()}-${m.groupValues[2]}", Scope.LIGHT)
    }

    // -- The tests --------------------------------------------------------

    @Test
    fun `every Color constant equals its CSS token`() {
        val css = parseCss()
        val constants = parseColorKt()
        val problems = mutableListOf<String>()

        for ((name, kotlinArgb) in constants) {
            val mapping = mappingFor(name)
            if (mapping == null) {
                problems += "$name has no mapping entry — a value the CSS doesn't have?"
                continue
            }
            val cssArgb = css.resolve(mapping)
            if (cssArgb == null) {
                problems += "$name maps ${mapping.token} (${mapping.scope}) but the CSS no longer has it"
                continue
            }
            if (cssArgb != kotlinArgb) {
                problems += "$name is ${argbHex(kotlinArgb)} but ${mapping.token} (${mapping.scope}) is ${argbHex(cssArgb)}"
            }
        }
        if (problems.isNotEmpty()) {
            fail("Color.kt has drifted from tokens/colors.css:\n  " + problems.joinToString("\n  "))
        }
    }

    @Test
    fun `rgba alpha rounds to nearest -- point-22 of crimson-500's base is 0x38`() {
        // .22*255 = 56.1 → 56 = 0x38; the other two shipped cases bracket
        // the rule from below (.12 → 30.6 → 31) and above (.07 → 17.85 → 18).
        assertEquals(0x38B3261EL, cssValueToArgb("rgba(179,38,30,.22)", emptyMap()))
        assertEquals(0x1FFFFFFFL, cssValueToArgb("rgba(255,255,255,.12)", emptyMap()))
        assertEquals(0x12FFFFFFL, cssValueToArgb("rgba(255,255,255,.07)", emptyMap()))
    }

    @Test
    fun `one level of var() indirection resolves`() {
        // No current constant needs this — every mapped token happens to be
        // a literal — but the first future mapping of a semantic alias like
        // light --status-danger-fg:var(--crimson-600) will. Load-bearing
        // for that entry, proven here so it can't rot invisibly.
        val css = parseCss()
        assertEquals(0xFF961D16L, css.resolve(ScopedToken("--status-danger-fg", Scope.LIGHT)))
    }

    // -- CSS parsing ------------------------------------------------------

    private enum class Scope { LIGHT, DARK }

    private data class ScopedToken(val token: String, val scope: Scope)

    private class Tokens(val light: Map<String, String>, val dark: Map<String, String>) {
        // Dark declarations fall back to :root for var() targets — the
        // ramps are declared once, in :root, and the dark scope aliases
        // into them (e.g. --surface-page:var(--ink-950)).
        fun resolve(st: ScopedToken): Long? {
            val scope = if (st.scope == Scope.DARK) dark else light
            val raw = scope[st.token] ?: return null
            return cssValueToArgb(raw, if (st.scope == Scope.DARK) light + dark else light)
        }
    }

    private fun parseCss(): Tokens {
        val css = repoFile(".claude/skills/hummingbird-design/tokens/colors.css")
            .readText()
            .replace(Regex("""/\*.*?\*/""", RegexOption.DOT_MATCHES_ALL), "")
        val light = mutableMapOf<String, String>()
        val dark = mutableMapOf<String, String>()
        // `:root` appears twice (ramps, then light semantics) and both land
        // in the same scope; `[data-theme="dark"]` is its own.
        for (block in Regex("""(:root|\[data-theme="dark"])\s*\{([^}]*)\}""").findAll(css)) {
            val into = if (block.groupValues[1] == ":root") light else dark
            for (decl in Regex("""(--[a-z0-9-]+)\s*:\s*([^;]+)""").findAll(block.groupValues[2])) {
                into[decl.groupValues[1]] = decl.groupValues[2].trim()
            }
        }
        check(light.isNotEmpty() && dark.isNotEmpty()) { "colors.css parsed to an empty scope — parser broken or file moved" }
        return Tokens(light, dark)
    }

    // -- Color.kt parsing -------------------------------------------------

    private fun parseColorKt(): Map<String, Long> {
        val lines = repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/ui/theme/Color.kt").readLines()
        val decl = Regex("""^val (\w+) = Color\(0x([0-9A-Fa-f]{8})\)""")
        val constants = lines.mapNotNull { decl.find(it) }
            .associate { it.groupValues[1] to it.groupValues[2].toLong(16) }
        check(constants.size >= 30) { "only ${constants.size} constants parsed from Color.kt — regex or file drifted" }
        return constants
    }

    private fun repoFile(relative: String): File {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle, or set the property (see app/build.gradle.kts)")
        val f = File(root, relative)
        check(f.isFile) { "$relative not found under $root" }
        return f
    }

    private fun argbHex(v: Long) = "0x%08X".format(v)

    companion object {
        // #rrggbb → 0xFFrrggbb; rgba(r,g,b,a) → alpha = round(a*255);
        // var(--x) → one level of lookup in the given scope.
        fun cssValueToArgb(raw: String, scope: Map<String, String>): Long {
            val value = raw.trim()
            Regex("""^var\((--[a-z0-9-]+)\)$""").matchEntire(value)?.let { m ->
                val target = scope[m.groupValues[1]]
                    ?: error("${m.groupValues[1]} (via var()) is not in scope")
                return cssValueToArgb(target, emptyMap()) // one level only, by design
            }
            Regex("""^#([0-9a-fA-F]{6})$""").matchEntire(value)?.let { m ->
                return 0xFF000000L or m.groupValues[1].toLong(16)
            }
            Regex("""^rgba\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*(0?\.\d+|[01])\s*\)$""").matchEntire(value)?.let { m ->
                val (r, g, b) = m.groupValues.slice(1..3).map { it.toLong() }
                val alpha = Math.round(m.groupValues[4].toDouble() * 255)
                return (alpha shl 24) or (r shl 16) or (g shl 8) or b
            }
            error("unhandled CSS value form: '$value'")
        }
    }
}
