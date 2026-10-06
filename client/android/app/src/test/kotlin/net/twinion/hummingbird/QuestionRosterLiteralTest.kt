package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// #716's gate, and ADR-0034 decision 4's premise on this client: every
// standing question's label, its place in the order, and the binding keys
// that answer it are the core's (`decisions/questions.rs`), crossed through
// `questionRoster()`. A Kotlin copy of any of them compiles, runs and reads
// right on the day it is written — and an eleventh question, or a reworded
// one, then needs a second edit nobody will remember.
//
// The words are read out of the core's own source rather than listed here,
// so this test cannot drift behind a rewording either. It scans every
// module's main Kotlin (app, wear, the shared libraries), with comments
// stripped so a doc may still name what it forbids.
class QuestionRosterLiteralTest {

    private val root: File by lazy {
        File(
            System.getProperty("hummingbird.repoRoot")
                ?: error("hummingbird.repoRoot not set — run under Gradle (see client/android/build.gradle.kts)"),
        )
    }

    private fun coreSource(relative: String): String {
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }

    /** One Rust `fn`'s body — from its signature to the first line that is
     * a lone closing brace. */
    private fun rustFn(src: String, name: String): String =
        Regex("""pub fn $name\([\s\S]*?\n}""").find(src)?.value
            ?: error("could not locate $name — has it been renamed?")

    private val labels: List<String> by lazy {
        val body = rustFn(coreSource("client/core/src/decisions/questions.rs"), "question_label")
        Regex("""StandingQuestion::\w+ => "([^"]+)"""").findAll(body).map { it.groupValues[1] }.toList()
    }

    private val bindingKeys: List<String> by lazy {
        val body = rustFn(coreSource("client/core/src/bindings.rs"), "as_str")
        Regex("""BindingKey::\w+ => "([^"]+)"""").findAll(body).map { it.groupValues[1] }.toList()
    }

    private val kotlinSources: List<Pair<String, String>> by lazy {
        File(root, "client/android").walkTopDown()
            .filter { it.isFile && it.extension == "kt" && "/src/main/" in it.path }
            .map { file ->
                file.relativeTo(root).path to file.readText()
                    .replace(Regex("""/\*[\s\S]*?\*/"""), "")
                    .replace(Regex("""(?m)^\s*//.*$"""), "")
            }
            .toList()
    }

    @Test
    fun `the gate reads real words out of the core`() {
        // Guards the guard: a regex that stopped matching would pass every
        // assertion below vacuously.
        assertTrue("expected the roster's eleven labels, read ${labels.size}", labels.size >= 11)
        assertTrue("expected the binding keys, read ${bindingKeys.size}", bindingKeys.size >= 5)
        assertTrue("expected Kotlin sources to scan", kotlinSources.any { it.first.endsWith("SettingsScreen.kt") })
    }

    @Test
    fun `no question label is spelled in Kotlin`() {
        for ((path, src) in kotlinSources) {
            for (label in labels) {
                assertFalse(
                    "$path spells the question label \"$label\" — read it from questionRoster()",
                    src.contains("\"$label"),
                )
            }
        }
    }

    @Test
    fun `no binding key is spelled in Kotlin`() {
        for ((path, src) in kotlinSources) {
            for (key in bindingKeys) {
                assertFalse(
                    "$path spells the binding key \"$key\" — the roster names which keys answer a question",
                    src.contains("\"$key\""),
                )
            }
        }
    }

    @Test
    fun `no question order is spelled in Kotlin`() {
        val literalList = Regex("""(listOf|setOf|arrayOf|mapOf|linkedSetOf|mutableListOf)\(\s*MobileStandingQuestion\.""")
        for ((path, src) in kotlinSources) {
            assertFalse(
                "$path lists standing questions in an order of its own — the roster's order is the core's",
                literalList.containsMatchIn(src),
            )
        }
    }
}
