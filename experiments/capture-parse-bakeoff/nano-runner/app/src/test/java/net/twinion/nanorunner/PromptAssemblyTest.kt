package net.twinion.nanorunner

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The bake-off's load-bearing test. If this goes red the Nano column is meaningless,
 * because the two parsers were no longer asked the same question.
 *
 * golden_prompts.jsonl is committed and regenerated with:
 *     ./run_hosted.py --emit-prompts --out nano-runner/app/src/test/resources/golden_prompts.jsonl
 */
class PromptAssemblyTest {

    private val harnessDir = File(requireNotNull(System.getProperty("harness.dir")))
    private val assetsDir = File(requireNotNull(System.getProperty("harness.assets")))

    private val golden: List<JsonObject> =
        requireNotNull(javaClass.classLoader!!.getResourceAsStream("golden_prompts.jsonl"))
            .bufferedReader()
            .readLines()
            .filter { it.isNotBlank() }
            .map { Json.parseToJsonElement(it) as JsonObject }

    private fun asset(name: String) = File(assetsDir, name).readText()

    @Test
    fun `bundled assets are byte-identical to the harness originals`() {
        for (name in listOf("corpus.jsonl", "prompt.md", "schema.json")) {
            val original = File(harnessDir, name)
            val copied = File(assetsDir, name)
            assertTrue("missing bundled asset $name", copied.isFile)
            assertTrue(
                "$name drifted from ${original.path} — the app would be asking a " +
                    "different question than the hosted side did",
                original.readBytes().contentEquals(copied.readBytes()),
            )
        }
    }

    @Test
    fun `golden file covers the corpus, same ids in the same order`() {
        val corpusIds = Corpus.parse(asset("corpus.jsonl")).map { it.id }
        val goldenIds = golden.map { it["id"]!!.jsonPrimitive.content }
        assertEquals(
            "golden_prompts.jsonl is stale — regenerate it (see nano-runner/README.md)",
            corpusIds,
            goldenIds,
        )
        assertEquals(42, corpusIds.size)
    }

    @Test
    fun `assembled prompt byte-matches run_hosted for every capture`() {
        val template = asset("prompt.md")
        val schemaFile = asset("schema.json")
        val captures = Corpus.parse(asset("corpus.jsonl")).associateBy { it.id }

        for (row in golden) {
            val id = row["id"]!!.jsonPrimitive.content
            val capture = requireNotNull(captures[id]) { "$id not in corpus" }
            // The golden row carries the raw the Python side used; if that disagrees with
            // the corpus we parsed, the drift is in the parse, not the assembly.
            assertEquals("$id: raw text differs", row["raw"]!!.jsonPrimitive.content, capture.raw)
            assertEquals(
                "$id: assembled prompt differs from run_hosted --emit-prompts",
                row["prompt"]!!.jsonPrimitive.content,
                PromptAssembly.assembleFromFiles(template, schemaFile, capture.raw),
            )
        }
    }

    @Test
    fun `no placeholder survives assembly`() {
        val template = asset("prompt.md")
        val schemaFile = asset("schema.json")
        for (capture in Corpus.parse(asset("corpus.jsonl"))) {
            val prompt = PromptAssembly.assembleFromFiles(template, schemaFile, capture.raw)
            assertTrue(
                "${capture.id}: unsubstituted placeholder left in prompt",
                !prompt.contains(PromptAssembly.SCHEMA_PLACEHOLDER) &&
                    !prompt.contains(PromptAssembly.RAW_PLACEHOLDER),
            )
        }
    }
}
