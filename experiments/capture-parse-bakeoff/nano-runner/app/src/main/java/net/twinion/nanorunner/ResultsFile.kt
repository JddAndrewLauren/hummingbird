package net.twinion.nanorunner

import java.io.Closeable
import java.io.File
import java.io.FileOutputStream
import java.io.RandomAccessFile
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonPrimitive

/** The results file on disk is inconsistent in a way we refuse to guess about. */
class CorruptResultsException(message: String) : Exception(message)

/**
 * An append-only JSONL file that survives the app being killed mid-run.
 *
 * Every row is written, flushed and `fd.sync()`ed before the next capture starts, so a
 * process death costs at most the in-flight inference. On the next launch:
 *
 *  - a torn FINAL line (a write interrupted by the kill) is truncated away — it is a
 *    crash artifact, not a result;
 *  - an unparseable line anywhere ELSE means the file is not what we think it is, and
 *    the run refuses to start rather than append into a file it can't account for;
 *  - every id already present is skipped, INCLUDING errored ones. An error is a result
 *    for the bake-off; silently retrying it would launder a failure into a success and
 *    quietly bias the Nano column.
 */
class ResultsFile(val file: File) : Closeable {

    private var out: FileOutputStream? = null

    /**
     * Repair a torn tail if there is one, then return the ids already recorded, in order.
     * @throws CorruptResultsException if a non-final line is unparseable.
     */
    fun repairAndScanIds(): List<String> {
        if (!file.isFile || file.length() == 0L) return emptyList()

        val bytes = file.readBytes()
        // Split on '\n', tracking each line's start offset so we can truncate precisely.
        val lines = mutableListOf<Pair<Long, String>>() // offset -> line (no terminator)
        var start = 0
        for (i in bytes.indices) {
            if (bytes[i] == '\n'.code.toByte()) {
                lines.add(start.toLong() to String(bytes, start, i - start, Charsets.UTF_8))
                start = i + 1
            }
        }
        val trailing = if (start < bytes.size) {
            start.toLong() to String(bytes, start, bytes.size - start, Charsets.UTF_8)
        } else {
            null
        }

        val ids = mutableListOf<String>()
        for ((offset, line) in lines) {
            if (line.isBlank()) continue
            val id = idOf(line)
                ?: throw CorruptResultsException(
                    "${file.name}: unparseable line at byte $offset, and it is not the " +
                        "last line — refusing to append to a file I can't account for. " +
                        "Move it aside and re-run to start clean."
                )
            ids.add(id)
        }

        // An unterminated tail means a write was cut short. If what landed still parses,
        // it is a whole result that merely lost its newline — keep it and finish the
        // write, because re-running that capture would either duplicate its id or (for
        // an error row) retry a failure we promised never to retry. If it doesn't parse,
        // it's a fragment: truncate it away.
        if (trailing != null) {
            val (offset, line) = trailing
            val id = if (line.isBlank()) null else idOf(line)
            if (id != null) {
                ids.add(id)
                FileOutputStream(file, true).use {
                    it.write("\n".toByteArray(Charsets.UTF_8))
                    it.flush()
                    it.fd.sync()
                }
            } else {
                RandomAccessFile(file, "rw").use { it.setLength(offset) }
            }
        }
        return ids
    }

    private fun idOf(line: String): String? = runCatching {
        (Json.parseToJsonElement(line) as JsonObject)["id"]!!.jsonPrimitive.content
    }.getOrNull()

    /** Append one already-serialised JSON row and force it to stable storage. */
    fun appendRow(json: String) {
        require(!json.contains('\n')) { "a JSONL row may not contain a newline" }
        val stream = out ?: FileOutputStream(file, true).also { out = it }
        stream.write((json + "\n").toByteArray(Charsets.UTF_8))
        stream.flush()
        stream.fd.sync()
    }

    /**
     * Remove the last [n] rows. Used only when the circuit breaker trips: the rows that
     * revealed a systemic condition are evidence about the phone, not results about
     * those captures, and leaving them in would spend those ids permanently — the same
     * reasoning that stops the run in the first place.
     */
    fun dropLastRows(n: Int) {
        if (n <= 0 || !file.isFile) return
        close() // reopen on the next append, at the new end
        val kept = file.readLines().filter { it.isNotBlank() }.dropLast(n)
        val text = if (kept.isEmpty()) "" else kept.joinToString("\n", postfix = "\n")
        FileOutputStream(file).use {
            it.write(text.toByteArray(Charsets.UTF_8))
            it.flush()
            it.fd.sync()
        }
    }

    override fun close() {
        out?.close()
        out = null
    }
}
