package net.twinion.hummingbird.skills

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// `SkillRunnerTest` drives the transport against **fake** reducers, because
// a plain JVM process cannot call a uniffi function. That leaves exactly one
// hole a green test run would not notice: a production wiring that also
// passes fakes, or passes nothing at all. This closes it mechanically — the
// `CaptureSubmitRefusalTest` pattern, which exists for the same reason on
// the capture surface.
class SkillsSeamTest {

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }

    private val runnerSrc by lazy {
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/skills/SkillRunner.kt")
    }

    @Test
    fun `every reducer default is the real uniffi binding`() {
        // The eight doors `ffi-mobile/src/lib.rs`'s M4 section exports for
        // this lane. A default that quietly became a Kotlin lambda would be
        // a second copy of the rule — ADR-0025's whole subject.
        for (binding in listOf(
            "grillTurnIdle",
            "grillTurnStarted",
            "grillTurnLine",
            "grillTurnNoToken",
            "grillTurnTransportFailed",
            "grillTurnResponseFailed",
            "grillTurnStreamEnded",
            "grillRunBody",
        )) {
            assertTrue(
                "SkillRunner.kt must import the generated $binding binding",
                runnerSrc.contains("import uniffi.hummingbird_ffi_mobile.$binding"),
            )
            assertTrue(
                "GrillReducers must default to ::$binding",
                runnerSrc.contains("::$binding"),
            )
        }
    }

    @Test
    fun `the production factory wires the real token store and the default reducers`() {
        val factory = Regex("""fun create\(context: Context\)[\s\S]*?\n\n""")
            .find(runnerSrc)
            ?.value
            ?: error("could not locate SkillRunner.create in the source")
        assertTrue(
            "create() must read the real device token",
            factory.contains("TokenStore.load(context)"),
        )
        // No `reducers =` override: the constructor default is
        // `GrillReducers()`, whose own defaults the test above pins.
        assertFalse(
            "create() must not substitute its own reducers",
            factory.contains("reducers ="),
        )
    }

    /** The compile-time drift gate `NowScreenStructuralTest` established
     * over `MobileUrgencyBand`: a `when` over a uniffi enum with an
     * `else ->` keeps compiling when the core grows a phase, and silently
     * drops it on the floor. Nothing in this lane may have one. */
    @Test
    fun `no when over a uniffi turn state carries an else arm`() {
        val root = System.getProperty("hummingbird.repoRoot")!!
        val dir = File(root, "client/android/app/src/main/kotlin/net/twinion/hummingbird/skills")
        for (file in dir.walkTopDown().filter { it.isFile && it.extension == "kt" }) {
            val body = file.readText()
            for (match in Regex("""when\s*\([^)]*\)\s*\{""").findAll(body)) {
                val tail = body.substring(match.range.last)
                val block = tail.substringBefore("\n    }")
                assertFalse(
                    "${file.name}: a when in this lane must be exhaustive with no else arm",
                    block.contains("else ->"),
                )
            }
        }
    }

    @Test
    fun `the transport builds no request body of its own`() {
        // The body is `grillRunBody`'s string, posted verbatim — the shared
        // fixture (`client/core/tests/fixtures/skills-run-bodies.json`) is
        // what pins its bytes, in Rust, TypeScript and the instrumented
        // suite. A body assembled here would escape all three.
        assertTrue(
            "the posted body must come from the reducers' runBody",
            Regex("""reducers\.runBody\([^)]*\)\.toRequestBody""").containsMatchIn(runnerSrc),
        )
        assertFalse("no hand-built JSON body", runnerSrc.contains("\"skill\":"))
        assertFalse("no hand-built JSON body", runnerSrc.contains("\"args\":"))
    }
}
