package net.twinion.nanorunner

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/**
 * Row shapes for the two output files.
 *
 * `nano_results.jsonl` mirrors `hosted_results.jsonl` exactly for the rows that worked
 * (`{"id","parse"}`) so the scoring path is the same on both sides. Rows that did not
 * work are recorded as results too (`{"id","error","raw_output"}`) — never retried,
 * never dropped. A failure IS a bake-off finding.
 */
object Rows {

    /**
     * Strict JSON: `kotlinx.serialization`, not `org.json`. Android's JSONTokener is
     * documented-lenient — it accepts single quotes, unquoted keys and trailing garbage
     * — and would launder a malformed model output into a valid-looking parse before
     * the Python validator ever saw it. The whole point is to measure what the model
     * actually emits. No trimming, no fence-stripping: a fenced or prose-wrapped answer
     * is a failure, and scoring gets to classify it.
     */
    private val strict = Json

    const val NOT_A_JSON_OBJECT = "model output is not a strict JSON object"

    /** @return the parsed object, or null if [text] is not strictly one JSON object. */
    fun parseStrictObject(text: String): JsonObject? =
        runCatching { strict.parseToJsonElement(text) as? JsonObject }.getOrNull()

    fun successRow(id: String, parse: JsonObject): String =
        buildJsonObject {
            put("id", id)
            put("parse", parse)
        }.toString()

    fun errorRow(id: String, error: String, rawOutput: String?): String =
        buildJsonObject {
            put("id", id)
            put("error", error)
            put("raw_output", rawOutput?.let { JsonPrimitive(it) } ?: JsonNull)
        }.toString()

    /** Audit sidecar: verbatim bytes for EVERY row, success or not. Merge never reads it. */
    fun rawRow(id: String, rawOutput: String?, latencyMs: Long): String =
        buildJsonObject {
            put("id", id)
            put("raw_output", rawOutput?.let { JsonPrimitive(it) } ?: JsonNull)
            put("latency_ms", latencyMs)
        }.toString()

    /** A one-line, self-describing rendering of whatever went wrong. */
    fun describe(t: Throwable): String {
        val code = genAiErrorCode(t)?.let { " code=$it" } ?: ""
        val message = t.message?.replace('\n', ' ')?.trim().orEmpty()
        return buildString {
            append(t.javaClass.name)
            append(code)
            if (message.isNotEmpty()) append(": ").append(message)
        }
    }

    private fun genAiErrorCode(t: Throwable): Int? = runCatching {
        (t as? com.google.mlkit.genai.common.GenAiException)?.errorCode
    }.getOrNull()
}
