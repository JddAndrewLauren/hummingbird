package net.twinion.hummingbird.skills

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The JVM twin of the web's `skills/no-queue.test.ts`, in the house
// source-parsing style (`hummingbird.repoRoot`, comments stripped before
// matching) that `NowItemDoorTest` and `DictationLocalityTest` already use.
//
// #273's acceptance says an unreachable or failed run puts **nothing** in
// the sync queue, the pending-mutation overlay or the dead-letter journal,
// and introduces **no new timer**. An assertion over an empty queue proves
// nothing about a lane that must not exist: the queue would be empty
// whether or not this code could reach it. What can be proved is that the
// lane is *physically unable* to reach any of them — an import-graph fact,
// and this repo already accepts source pins for that class of invariant.
//
// **It walks the package directory** rather than carrying a hand-written
// module list, which is the one place it improves on the web original: a
// new file in this package cannot silently escape the gate.
class SkillsLaneIsolationTest {

    private val laneFiles: List<Pair<String, String>> by lazy {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val dir = File(root, "client/android/app/src/main/kotlin/net/twinion/hummingbird/skills")
        check(dir.isDirectory) { "the skills package was not found under $root" }
        val files = dir.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
        check(files.isNotEmpty()) { "the skills package has no Kotlin files — the gate would pass vacuously" }
        files.map { it.name to code(it.readText()) }
    }

    /** Comments discuss all of this at length; only code counts. */
    private fun code(source: String): String =
        source
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .lines()
            .filterNot { it.trimStart().startsWith("//") }
            .joinToString("\n")

    @Test
    fun `no file in the lane can reach the sync engine or the core holder`() {
        for ((name, body) in laneFiles) {
            assertFalse(
                "$name imports the sync package — a skill request is a question, not a queued fact",
                body.contains("net.twinion.hummingbird.sync"),
            )
            assertFalse("$name references CoreHolder", body.contains("CoreHolder"))
            assertFalse("$name references SyncWorker", body.contains("SyncWorker"))
            assertFalse("$name references WorkManager", body.contains("WorkManager"))
            assertFalse("$name references the dead-letter journal", body.contains("deadLetter"))
            assertFalse("$name references the dead-letter journal", body.contains("DeadLetter"))
        }
    }

    @Test
    fun `nothing in the lane hand-rolls a cadence`() {
        // OkHttp's own connect/read deadlines are the sanctioned exception —
        // exactly as `AbortSignal.timeout` is on the web (ADR-0007's #274
        // amendment). What ADR-0007 bans is a *cadence*: something that
        // repeats, reschedules or polls. A deadline arms once per request
        // and disarms when it settles, and the client holds it, not this
        // code. Building the same thing by hand out of a Timer, a Handler or
        // a `delay` loop is still banned, and this is what catches it.
        val banned = listOf(
            "Timer(",
            "fixedRateTimer",
            "ScheduledExecutorService",
            "Handler(",
            "postDelayed",
            "delay(",
            ".retry(",
            "retryWhen",
            "AlarmManager",
        )
        for ((name, body) in laneFiles) {
            for (spelling in banned) {
                assertFalse("$name uses $spelling — the lane has no cadence of its own", body.contains(spelling))
            }
        }
    }

    @Test
    fun `no decline prose and no provider name is spelled in Kotlin`() {
        // The words come from `hummingbird_core::decisions::skills::decline`
        // or not at all, and the stamp is read off the envelope or is
        // absent — so no file here may contain either. This is the Kotlin
        // side of the same rule `no-queue.test.ts` pins over `ItemPanel.tsx`
        // and `ffi-mobile`'s own `no_decline_or_stamp_names_a_provider`.
        val declineSentences = listOf(
            "No device token on this device",
            "The run ended without an answer",
            "Could not reach the server",
            "was rejected. Re-enter it",
            "not allowed to run skills",
            "The server answered",
            "answered outside the schema",
        )
        val providerNames = listOf("anthropic", "claude-", "sonnet", "opus", "haiku", "moonshot")
        for ((name, body) in laneFiles) {
            for (sentence in declineSentences) {
                assertFalse("$name spells a decline sentence: $sentence", body.contains(sentence))
            }
            val lowered = body.lowercase()
            for (provider in providerNames) {
                assertFalse("$name names a backend or model: $provider", lowered.contains(provider))
            }
        }
    }

    @Test
    fun `nothing in the lane re-derives what a runner line means`() {
        // Classification is the core's (`skills::envelope::classify_line`)
        // and crosses as an applied result. A JSON parser anywhere in this
        // package would be a second reader of the wire grammar — the exact
        // second copy ADR-0025 exists to prevent.
        for ((name, body) in laneFiles) {
            assertFalse("$name parses JSON with JSONObject", body.contains("JSONObject"))
            assertFalse("$name parses JSON with JSONArray", body.contains("JSONArray"))
            assertFalse("$name parses JSON with kotlinx.serialization", body.contains("kotlinx.serialization"))
            assertFalse("$name parses JSON with Gson", body.contains("Gson"))
            assertFalse("$name reads the ok field itself", body.contains("\"ok\""))
        }
    }

    @Test
    fun `the lane really is more than an empty directory`() {
        // The walk above is only a gate if it found the transport; a
        // renamed package would otherwise turn every assertion vacuous.
        assertTrue(
            "SkillRunner.kt must be part of the walked lane",
            laneFiles.any { (name, _) -> name == "SkillRunner.kt" },
        )
    }
}
