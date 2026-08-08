package net.twinion.nanorunner

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonPrimitive

/** One capture from corpus.jsonl. `raw` is verbatim — never cleaned, never trimmed. */
data class Capture(val id: String, val raw: String, val source: String?)

object Corpus {

    /**
     * Parse corpus.jsonl. Strict: a malformed line is a build/asset error, not something
     * to skip past — a silently shortened corpus would score as a smaller bake-off.
     */
    fun parse(text: String): List<Capture> {
        val captures = text.lineSequence()
            .filter { it.isNotBlank() }
            .mapIndexed { i, line ->
                val obj = Json.parseToJsonElement(line) as? JsonObject
                    ?: error("corpus.jsonl:${i + 1}: line is not a JSON object")
                val id = obj["id"]?.jsonPrimitive?.content
                    ?: error("corpus.jsonl:${i + 1}: missing 'id'")
                val raw = obj["raw"]?.jsonPrimitive?.content
                    ?: error("corpus.jsonl:${i + 1}: missing 'raw'")
                val source = (obj["source"] as? JsonPrimitive)?.takeIf { it !is JsonNull }?.content
                Capture(id, raw, source)
            }
            .toList()
        val dupes = captures.groupBy { it.id }.filterValues { it.size > 1 }.keys
        require(dupes.isEmpty()) { "corpus.jsonl: duplicate id(s): ${dupes.joinToString()}" }
        return captures
    }
}
