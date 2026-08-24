package net.twinion.hummingbird.diagnostics

import java.io.File
import java.nio.file.Files
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** [DiagnosticJournal]'s own suite (#709) — plain `java.io.File` work, no
 * Android framework and no native `.so`, so this runs as an ordinary JUnit
 * test with a real temp directory, the same shape `FsSnapshotStore`'s own
 * Rust tests take with `tempdir()`. Assertions are on the raw JSON text
 * rather than a parsed structure: this app carries no JSON-parsing
 * dependency (see the class's own doc for why it does not need one), and
 * adding one just for this test would be exactly the kind of scope this
 * slice's brief does not ask for. */
class DiagnosticJournalTest {

    private lateinit var dir: File

    @Before
    fun setUp() {
        dir = Files.createTempDirectory("diagnostic-journal-test").toFile()
    }

    @After
    fun tearDown() {
        dir.deleteRecursively()
    }

    private fun event(name: String, seq: Int): String =
        """{"schema_version":1,"seq":$seq,"session_id":"s-1","source":"android","event":{"name":"$name"}}"""

    @Test
    fun `an empty journal exports zero events and a zero dropped count`() {
        val exported = String(DiagnosticJournal(dir).export())

        assertEquals("""{"schema_version":1,"dropped_count":0,"events":[]}""", exported)
    }

    @Test
    fun `append then export round trips the event verbatim`() {
        val journal = DiagnosticJournal(dir)
        val pushReceived = event("push.received", 0)

        journal.append(pushReceived, wallClockMs = 1_000)

        val exported = String(journal.export())
        assertEquals("""{"schema_version":1,"dropped_count":0,"events":[$pushReceived]}""", exported)
    }

    @Test
    fun `a fresh journal instance over the same directory sees what the previous instance wrote`() {
        // Simulates process death: no in-memory state survives, only the
        // files on disk — a second `DiagnosticJournal` over the same
        // directory is exactly what a cold-started process constructs.
        DiagnosticJournal(dir).append(event("worker.started", 0), wallClockMs = 1_000)

        val reloaded = DiagnosticJournal(dir)
        assertEquals(1, reloaded.events().size)
        assertTrue(reloaded.events()[0].contains("worker.started"))
    }

    @Test
    fun `events export in the order they were appended, across a reload`() {
        DiagnosticJournal(dir).append(event("worker.started", 0), wallClockMs = 1_000)
        DiagnosticJournal(dir).append(event("worker.finished", 1), wallClockMs = 1_100)
        DiagnosticJournal(dir).append(event("push.received", 2), wallClockMs = 1_200)

        val events = DiagnosticJournal(dir).events()
        assertEquals(3, events.size)
        assertTrue(events[0].contains("worker.started"))
        assertTrue(events[1].contains("worker.finished"))
        assertTrue(events[2].contains("push.received"))
    }

    @Test
    fun `a truncated final line costs one event, not the whole file`() {
        val journal = DiagnosticJournal(dir)
        journal.append(event("worker.started", 0), wallClockMs = 1_000)
        // Simulate a process killed mid-`appendText`: a second line with a
        // timestamp prefix but no closing brace.
        File(dir, "diagnostics.ndjson").appendText("1100\t{\"schema_version\":1,\"seq\":1,\"event\":{\"nam")

        val events = DiagnosticJournal(dir).events()
        assertEquals(1, events.size)
        assertTrue(events[0].contains("worker.started"))
    }

    @Test
    fun `the size limit rotates the oldest events and raises the dropped count`() {
        // A tiny size budget: each stored line here is on the order of 90
        // bytes, so a 150-byte budget keeps roughly one event.
        val journal = DiagnosticJournal(dir, maxSizeBytes = 150)
        journal.append(event("worker.started", 0), wallClockMs = 1_000)
        journal.append(event("worker.finished", 1), wallClockMs = 1_001)
        journal.append(event("push.received", 2), wallClockMs = 1_002)

        assertTrue(journal.droppedCount() > 0)
        // The most recent event always survives a size-only rotation.
        assertTrue(journal.events().last().contains("push.received"))
    }

    @Test
    fun `the age limit rotates events older than the cutoff and raises the dropped count`() {
        val journal = DiagnosticJournal(dir, maxAgeMs = 1_000)
        journal.append(event("worker.started", 0), wallClockMs = 0)
        // Old enough to be dropped once "now" is far enough past it.
        journal.append(event("push.received", 1), wallClockMs = 5_000)

        assertEquals(1L, journal.droppedCount())
        assertEquals(1, journal.events().size)
        assertTrue(journal.events()[0].contains("push.received"))
    }

    @Test
    fun `the dropped count is cumulative across more than one rotation, not reset by the later one`() {
        // Exact values, not a `>=`: with this fixture and a 150-byte
        // budget, each append after the first keeps exactly one surviving
        // event and drops exactly one more. A cumulative count reaches 1,
        // then 2, then 3 — a count that *resets* on each rotation would
        // read 1, then 1, then 1, and a loose `actual >= previous`
        // assertion cannot tell the two apart (1 >= 1 passes either way).
        // This is the brief's own named defect ("a dropped count that
        // resets on rotation is worse than none"), so this test pins the
        // exact running total at every step.
        val journal = DiagnosticJournal(dir, maxSizeBytes = 150)

        journal.append(event("worker.started", 0), wallClockMs = 1_000)
        assertEquals(0L, journal.droppedCount())

        journal.append(event("worker.finished", 1), wallClockMs = 1_001)
        assertEquals(1L, journal.droppedCount())

        journal.append(event("push.received", 2), wallClockMs = 1_002)
        assertEquals(2L, journal.droppedCount())

        journal.append(event("session.started", 3), wallClockMs = 1_003)
        assertEquals(3L, journal.droppedCount())
    }

    @Test
    fun `the dropped count itself survives a reload`() {
        val journal = DiagnosticJournal(dir, maxSizeBytes = 150)
        journal.append(event("worker.started", 0), wallClockMs = 1_000)
        journal.append(event("worker.finished", 1), wallClockMs = 1_001)
        journal.append(event("push.received", 2), wallClockMs = 1_002)
        val dropped = journal.droppedCount()
        assertTrue(dropped > 0)

        val reloaded = DiagnosticJournal(dir, maxSizeBytes = 150)
        assertEquals(dropped, reloaded.droppedCount())
    }

    @Test
    fun `clear empties both the journal and the dropped count`() {
        val journal = DiagnosticJournal(dir, maxSizeBytes = 150)
        journal.append(event("worker.started", 0), wallClockMs = 1_000)
        journal.append(event("worker.finished", 1), wallClockMs = 1_001)
        journal.append(event("push.received", 2), wallClockMs = 1_002)
        assertTrue(journal.droppedCount() > 0)

        journal.clear()

        assertEquals(0L, journal.droppedCount())
        assertTrue(journal.events().isEmpty())
        assertEquals("""{"schema_version":1,"dropped_count":0,"events":[]}""", String(journal.export()))
    }

    @Test
    fun `the export never contains the sibling mirror file's content`() {
        // Same directory, deliberately: `Core::init` (`client/core/src/
        // lib.rs`) lays `mirror.json`/`queue.json`/`grill-drafts.json`
        // down alongside the journal in this exact directory — `export`
        // must read only its own `diagnostics.ndjson`/
        // `diagnostics.dropped-count`, never those. (The device token is
        // not a sibling here at all — it lives in `TokenStore`'s
        // `EncryptedSharedPreferences`, outside this directory entirely,
        // so it has no file here to leak from in the first place.)
        File(dir, "mirror.json").writeText("""{"items":[{"title":"a private item title"}]}""")

        val journal = DiagnosticJournal(dir)
        journal.append(event("push.received", 0), wallClockMs = 1_000)

        assertFalse(String(journal.export()).contains("a private item title"))
    }

    @Test
    fun `clear never touches sibling files in the same core directory`() {
        // The journal shares its directory with the core's own mirror,
        // outbound queue and grill drafts (`Core::init`'s own three
        // files) — `clear` must only ever remove its own two
        // `diagnostics.*` files.
        val mirrorFile = File(dir, "mirror.json").apply { writeText("mirror-bytes") }
        val queueFile = File(dir, "queue.json").apply { writeText("queue-bytes") }
        val grillDraftsFile = File(dir, "grill-drafts.json").apply { writeText("grill-drafts-bytes") }

        val journal = DiagnosticJournal(dir)
        journal.append(event("push.received", 0), wallClockMs = 1_000)
        journal.clear()

        assertEquals("mirror-bytes", mirrorFile.readText())
        assertEquals("queue-bytes", queueFile.readText())
        assertEquals("grill-drafts-bytes", grillDraftsFile.readText())
    }

    @Test
    fun `a write into a directory append cannot create throws, which the recorder is what swallows`() {
        // DiagnosticJournal itself is a plain, honest file writer: proving
        // a caller is unaffected by a failure in here is DiagnosticsRecorder's
        // own job (its own suite, `record swallows a journal failure`).
        // Pinning that this class *can* throw is what makes that
        // swallowing claim non-vacuous rather than trivially true.
        val blocker = File(dir, "blocked")
        blocker.writeText("a file, not a directory")
        val blockedJournal = DiagnosticJournal(File(blocker, "core"))

        var threw = false
        try {
            blockedJournal.append(event("push.received", 0), wallClockMs = 1_000)
        } catch (_: Exception) {
            threw = true
        }
        assertTrue(threw)
        assertFalse(File(blocker, "core/diagnostics.ndjson").exists())
    }
}
