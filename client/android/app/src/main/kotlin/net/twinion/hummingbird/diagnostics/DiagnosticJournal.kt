package net.twinion.hummingbird.diagnostics

import java.io.File
import java.nio.charset.StandardCharsets

/**
 * The on-disk NDJSON journal (#709), over `hummingbird_core::diagnostics`'s
 * `DiagnosticEventV1` wire shape (#706). Pure `java.io.File` work over a
 * caller-supplied directory — no coroutine, no Android framework
 * dependency — so this is exercised directly from a plain JVM unit test
 * with a temp directory, the same shape `core`'s own `FsSnapshotStore`
 * Rust tests take.
 *
 * **Storage encoding is not export encoding.** Each stored line is
 * `"$wallClockMs\t$eventJson\n"` — the tab-prefixed timestamp is this
 * class's own bookkeeping for the age half of retention, read back without
 * ever parsing the JSON payload itself: this app has no JSON-parsing
 * dependency, and does not need one here, because every `eventJson` half
 * is already complete, valid JSON minted by `diagnosticEventJson`
 * (`hummingbird-ffi-mobile`) and is copied byte-for-byte into the export.
 * A line with no tab, or whose JSON half does not both start with `{` and
 * end with `}`, is a truncated final line — the shape a process kill
 * mid-`appendText` leaves behind — and is dropped silently at read time,
 * costing that one event rather than the whole file.
 *
 * **Rotation, not compaction (the brief's own distinction).** [append]
 * checks both retention limits after every write and, if either is over
 * budget, drops the oldest surviving events — by age first, then by size —
 * rewriting the file atomically (temp-then-rename, `FsSnapshotStore`'s own
 * pattern) so a reader never observes a half-rewritten journal. The
 * dropped count is cumulative across every rotation this journal (this
 * directory) has ever done, in its own small sidecar file, because a count
 * that reset on rotation would read as "nothing was lost".
 */
class DiagnosticJournal(
    private val directory: File,
    private val maxAgeMs: Long = DEFAULT_MAX_AGE_MS,
    private val maxSizeBytes: Long = DEFAULT_MAX_SIZE_BYTES,
) {
    private val journalFile = File(directory, "diagnostics.ndjson")
    private val droppedCountFile = File(directory, "diagnostics.dropped-count")

    /** Appends one already-serialized `DiagnosticEventV1` JSON line
     * (verbatim — this class never re-encodes it) then rotates if the
     * append pushed the journal over either retention limit.
     * `wallClockMs` is the event's own timestamp, the same value the
     * caller already passed to mint `eventJson` — never re-derived by
     * parsing it back out. */
    @Synchronized
    fun append(eventJson: String, wallClockMs: Long) {
        directory.mkdirs()
        journalFile.appendText("$wallClockMs\t$eventJson\n", StandardCharsets.UTF_8)
        rotateIfNeeded(nowMs = wallClockMs)
    }

    /** The exported bytes: every surviving event plus the cumulative
     * dropped count, as one `application/json` document — logs only, never
     * the task mirror, the outbound queue or the device token, none of
     * which this class or its directory's `diagnostics.*` files ever
     * touch. */
    @Synchronized
    fun export(): ByteArray {
        val events = readEntries().joinToString(",") { it.json }
        val dropped = readDroppedCount()
        val body = "{\"schema_version\":1,\"dropped_count\":$dropped,\"events\":[$events]}"
        return body.toByteArray(StandardCharsets.UTF_8)
    }

    /** Empties the journal and its dropped count. Touches only this
     * journal's own two files. */
    @Synchronized
    fun clear() {
        journalFile.delete()
        droppedCountFile.delete()
    }

    /** Test-only: the cumulative dropped count without paying for a full
     * export. */
    @Synchronized
    internal fun droppedCount(): Long = readDroppedCount()

    /** Test-only: every surviving event's raw JSON, oldest first. */
    @Synchronized
    internal fun events(): List<String> = readEntries().map { it.json }

    private data class Entry(val wallClockMs: Long, val json: String)

    private fun readEntries(): List<Entry> {
        if (!journalFile.exists()) return emptyList()
        return journalFile.readLines(StandardCharsets.UTF_8).mapNotNull(::parseLine)
    }

    private fun parseLine(line: String): Entry? {
        val tab = line.indexOf('\t')
        if (tab < 0) return null
        val wallClockMs = line.substring(0, tab).toLongOrNull() ?: return null
        val json = line.substring(tab + 1)
        if (!json.startsWith("{") || !json.endsWith("}")) return null
        return Entry(wallClockMs, json)
    }

    private fun readDroppedCount(): Long =
        if (droppedCountFile.exists()) {
            droppedCountFile.readText(StandardCharsets.UTF_8).trim().toLongOrNull() ?: 0L
        } else {
            0L
        }

    private fun writeDroppedCount(count: Long) {
        val tmp = File(directory, "diagnostics.dropped-count.tmp")
        tmp.writeText(count.toString(), StandardCharsets.UTF_8)
        tmp.renameTo(droppedCountFile)
    }

    private fun rotateIfNeeded(nowMs: Long) {
        val entries = readEntries()
        if (entries.isEmpty()) return

        // Age: drop the oldest run of events past the cutoff.
        val cutoff = nowMs - maxAgeMs
        var dropCount = 0
        while (dropCount < entries.size && entries[dropCount].wallClockMs < cutoff) {
            dropCount++
        }

        // Size: keep dropping the oldest surviving event until the kept
        // set is back under budget.
        var size = entries.drop(dropCount).sumOf(::lineBytes)
        while (size > maxSizeBytes && dropCount < entries.size) {
            size -= lineBytes(entries[dropCount])
            dropCount++
        }

        if (dropCount == 0) return
        writeAtomically(entries.drop(dropCount))
        writeDroppedCount(readDroppedCount() + dropCount)
    }

    private fun lineBytes(entry: Entry): Int =
        "${entry.wallClockMs}\t${entry.json}\n".toByteArray(StandardCharsets.UTF_8).size

    private fun writeAtomically(entries: List<Entry>) {
        val tmp = File(directory, "diagnostics.ndjson.tmp")
        tmp.writeText(
            entries.joinToString("") { "${it.wallClockMs}\t${it.json}\n" },
            StandardCharsets.UTF_8,
        )
        tmp.renameTo(journalFile)
    }

    companion object {
        /** 72 hours — the brief's own age limit. */
        const val DEFAULT_MAX_AGE_MS: Long = 72L * 60 * 60 * 1000
        /** 10 MiB — the brief's own size limit. */
        const val DEFAULT_MAX_SIZE_BYTES: Long = 10L * 1024 * 1024
    }
}
