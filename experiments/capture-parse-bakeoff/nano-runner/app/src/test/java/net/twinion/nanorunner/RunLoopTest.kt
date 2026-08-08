package net.twinion.nanorunner

import java.io.File
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class RunLoopTest {

    @get:Rule val tmp = TemporaryFolder()

    private val captures = listOf(
        Capture("a", "raw a", null),
        Capture("b", "raw b", null),
        Capture("c", "raw c", null),
    )

    /** Returns a scripted output (or throws) per capture, and records what it was asked. */
    private class ScriptedEngine(private val script: Map<String, Any>) : NanoEngine {
        val prompts = mutableListOf<String>()
        override suspend fun availability() = Availability.AVAILABLE
        override fun download(): Flow<DownloadProgress> = emptyFlow()
        override suspend fun generate(prompt: String): String {
            prompts += prompt
            val key = script.keys.first { prompt.contains(it) }
            return when (val v = script.getValue(key)) {
                is Throwable -> throw v
                else -> v as String
            }
        }
        override suspend fun describe() = mapOf("engine" to "scripted")
        override fun close() = Unit
    }

    private fun rows(f: File): List<JsonObject> =
        f.readLines().filter { it.isNotBlank() }.map { Json.parseToJsonElement(it) as JsonObject }

    private fun loop(engine: NanoEngine, results: ResultsFile, raw: ResultsFile) = RunLoop(
        engine = engine,
        captures = captures,
        template = "SCHEMA={{SCHEMA}} RAW={{RAW}}",
        schemaText = "{}\n",
        results = results,
        raw = raw,
    )

    @Test
    fun `success, invalid output and thrown error are all recorded as results`() = runTest {
        val resultsF = File(tmp.root, "r.jsonl")
        val rawF = File(tmp.root, "raw.jsonl")
        val engine = ScriptedEngine(
            mapOf(
                "raw a" to "{\"title\":\"A\"}",
                "raw b" to "{'title': 'lenient'}", // org.json would accept this; we must not
                "raw c" to IllegalStateException("engine exploded"),
            )
        )

        val summary = ResultsFile(resultsF).use { r ->
            ResultsFile(rawF).use { raw -> loop(engine, r, raw).run() }
        }

        assertEquals(3, summary.total)
        assertEquals(1, summary.ok)
        assertEquals(2, summary.errors)
        assertEquals(listOf("b", "c"), summary.failedIds)

        val recorded = rows(resultsF)
        assertEquals(listOf("a", "b", "c"), recorded.map { it["id"]!!.jsonPrimitive.content })
        assertEquals("A", (recorded[0]["parse"] as JsonObject)["title"]!!.jsonPrimitive.content)
        assertEquals(Rows.NOT_A_JSON_OBJECT, recorded[1]["error"]!!.jsonPrimitive.content)
        assertEquals("{'title': 'lenient'}", recorded[1]["raw_output"]!!.jsonPrimitive.content)
        assertTrue(recorded[2]["error"]!!.jsonPrimitive.content.contains("engine exploded"))
        assertEquals(kotlinx.serialization.json.JsonNull, recorded[2]["raw_output"])

        // The audit sidecar has a row for every capture, verbatim, with a latency.
        assertEquals(listOf("a", "b", "c"), rows(rawF).map { it["id"]!!.jsonPrimitive.content })
    }

    @Test
    fun `a fenced answer is a failure, never unwrapped`() = runTest {
        val resultsF = File(tmp.root, "r.jsonl")
        val rawF = File(tmp.root, "raw.jsonl")
        val engine = ScriptedEngine(
            mapOf(
                "raw a" to "```json\n{\"title\":\"A\"}\n```",
                "raw b" to "{\"title\":\"B\"}",
                "raw c" to "Sure! Here is the JSON: {\"title\":\"C\"}",
            )
        )
        val summary = ResultsFile(resultsF).use { r ->
            ResultsFile(rawF).use { raw -> loop(engine, r, raw).run() }
        }
        assertEquals(1, summary.ok)
        assertEquals(listOf("a", "c"), summary.failedIds)
    }

    @Test
    fun `the prompt handed to the engine is the assembled one`() = runTest {
        val engine = ScriptedEngine(captures.associate { it.raw to "{\"title\":\"x\"}" })
        ResultsFile(File(tmp.root, "r.jsonl")).use { r ->
            ResultsFile(File(tmp.root, "raw.jsonl")).use { raw -> loop(engine, r, raw).run() }
        }
        assertEquals(listOf("SCHEMA={} RAW=raw a", "SCHEMA={} RAW=raw b", "SCHEMA={} RAW=raw c"),
            engine.prompts)
    }

    @Test
    fun `resume skips recorded ids, including errored ones`() = runTest {
        val resultsF = File(tmp.root, "r.jsonl")
        val rawF = File(tmp.root, "raw.jsonl")
        ResultsFile(resultsF).use {
            it.appendRow(Rows.successRow("a", Rows.parseStrictObject("{\"title\":\"A\"}")!!))
            it.appendRow(Rows.errorRow("b", "an earlier failure", null))
        }

        val engine = ScriptedEngine(mapOf("raw c" to "{\"title\":\"C\"}"))
        val summary = ResultsFile(resultsF).use { r ->
            ResultsFile(rawF).use { raw -> loop(engine, r, raw).run() }
        }

        assertEquals(2, summary.skipped)
        assertEquals(1, summary.ok)
        assertEquals(1, engine.prompts.size) // b was NOT retried
        val ids = rows(resultsF).map { it["id"]!!.jsonPrimitive.content }
        assertEquals(listOf("a", "b", "c"), ids)
        assertEquals(ids.size, ids.distinct().size)
        // The pre-existing error row survived untouched.
        assertEquals("an earlier failure", rows(resultsF)[1]["error"]!!.jsonPrimitive.content)
    }

    /**
     * The 2026-08-08 phone run: `maxOutputTokens = 1024` was rejected by AICore before
     * any inference, so all 42 captures failed identically and were recorded — and
     * recorded ids are never re-run. A config fault must abort with nothing written.
     */
    @Test
    fun `a misconfigured engine aborts the run and records nothing`() = runTest {
        val resultsF = File(tmp.root, "r.jsonl")
        val rawF = File(tmp.root, "raw.jsonl")
        val engine = object : NanoEngine {
            override suspend fun availability() = Availability.AVAILABLE
            override fun download(): Flow<DownloadProgress> = emptyFlow()
            override suspend fun generate(prompt: String): String =
                throw EngineMisconfigured(
                    "generation config rejected by the SDK",
                    IllegalArgumentException("maxOutputTokens must be between 1 and 256"),
                )
            override suspend fun describe() = emptyMap<String, String>()
            override fun close() = Unit
        }

        var thrown: EngineMisconfigured? = null
        try {
            ResultsFile(resultsF).use { r ->
                ResultsFile(rawF).use { raw -> loop(engine, r, raw).run() }
            }
        } catch (e: EngineMisconfigured) {
            thrown = e
        }

        assertTrue("the fault must surface, not be recorded", thrown != null)
        assertTrue("no id may be spent on our own config bug", resultsF.length() == 0L)
    }

    @Test
    fun `a systemic failure stops the run instead of spending every id on it`() = runTest {
        val resultsF = File(tmp.root, "r.jsonl")
        val rawF = File(tmp.root, "raw.jsonl")
        val many = (1..10).map { Capture("c$it", "raw $it", null) }
        // Every capture fails the same way, as a quota or thermal cutoff would.
        val engine = ScriptedEngine(many.associate { it.raw to IllegalStateException("BUSY") })

        val summary = ResultsFile(resultsF).use { r ->
            ResultsFile(rawF).use { raw ->
                RunLoop(engine, many, "{{RAW}}", "{}", r, raw).run()
            }
        }

        assertEquals(RunLoop.IDENTICAL_ERROR_LIMIT, summary.errors)
        assertTrue(summary.stoppedEarly != null)
        // The untouched ids are still runnable later — that's the whole point.
        assertEquals(RunLoop.IDENTICAL_ERROR_LIMIT, resultsF.readLines().size)
    }

    /**
     * The 2026-08-08 re-run: Nano wraps every answer in ```json fences, so every capture
     * failed the strict parse identically. That is a FINDING and we want it for all 42 —
     * the breaker guards the corpus against the phone, not against the model.
     */
    @Test
    fun `output the model did produce never trips the circuit breaker`() = runTest {
        val resultsF = File(tmp.root, "r.jsonl")
        val rawF = File(tmp.root, "raw.jsonl")
        val many = (1..10).map { Capture("c$it", "raw $it", null) }
        val engine = ScriptedEngine(
            many.associate { it.raw to "```json\n{\"title\":\"fenced\"}\n```" }
        )

        val summary = ResultsFile(resultsF).use { r ->
            ResultsFile(rawF).use { raw ->
                RunLoop(engine, many, "{{RAW}}", "{}", r, raw).run()
            }
        }

        assertEquals(null, summary.stoppedEarly)
        assertEquals(10, summary.errors)
        assertEquals(10, resultsF.readLines().size)
    }

    @Test
    fun `scattered unrelated failures do not trip the circuit breaker`() = runTest {
        val resultsF = File(tmp.root, "r.jsonl")
        val rawF = File(tmp.root, "raw.jsonl")
        val many = (1..9).map { Capture("c$it", "raw $it", null) }
        val engine = ScriptedEngine(
            many.associate { c ->
                val n = c.id.removePrefix("c").toInt()
                c.raw to if (n % 2 == 0) IllegalStateException("failure $n") else "{\"title\":\"ok\"}"
            }
        )

        val summary = ResultsFile(resultsF).use { r ->
            ResultsFile(rawF).use { raw ->
                RunLoop(engine, many, "{{RAW}}", "{}", r, raw).run()
            }
        }

        assertEquals(null, summary.stoppedEarly)
        assertEquals(9, resultsF.readLines().size)
    }

    @Test
    fun `a run killed mid-file resumes without duplicating or losing an id`() = runTest {
        val resultsF = File(tmp.root, "r.jsonl")
        val rawF = File(tmp.root, "raw.jsonl")
        // Simulate the kill: one good row, then a half-written second row.
        resultsF.writeText("{\"id\":\"a\",\"parse\":{\"title\":\"A\"}}\n{\"id\":\"b\",\"pa")

        val engine = ScriptedEngine(
            mapOf("raw b" to "{\"title\":\"B\"}", "raw c" to "{\"title\":\"C\"}")
        )
        ResultsFile(resultsF).use { r ->
            ResultsFile(rawF).use { raw -> loop(engine, r, raw).run() }
        }

        val ids = rows(resultsF).map { it["id"]!!.jsonPrimitive.content }
        assertEquals(listOf("a", "b", "c"), ids)
    }
}
