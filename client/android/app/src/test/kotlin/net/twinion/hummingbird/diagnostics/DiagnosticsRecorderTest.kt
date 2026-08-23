package net.twinion.hummingbird.diagnostics

import java.io.File
import java.nio.file.Files
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileDiagnosticEvent

/** [DiagnosticsRecorder]'s own suite (#709). `mintEventJsonFn` is always a
 * fixture here, never the real `diagnosticEventJson` — that function is a
 * `#[uniffi::export]` call into the native `.so`, unreachable from a plain
 * JVM unit test (the same reason `SettingsViewModel` injects
 * `deadLetterHeadingFn`). The fixture's shape is not a guess: it mirrors
 * what `hummingbird-ffi-mobile`'s own `diagnostics::tests` (Rust) already
 * pins byte-for-byte for each event kind this class mints. */
class DiagnosticsRecorderTest {

    private lateinit var dir: File

    @Before
    fun setUp() {
        dir = Files.createTempDirectory("diagnostics-recorder-test").toFile()
    }

    @After
    fun tearDown() {
        dir.deleteRecursively()
    }

    private fun fixtureJson(event: MobileDiagnosticEvent, wallClockMs: Long, @Suppress("UNUSED_PARAMETER") monotonicMs: Long): String {
        val (eventName, payload) = when (event) {
            is MobileDiagnosticEvent.SessionStarted -> "session.started" to "null"
            is MobileDiagnosticEvent.WorkerStarted -> "worker.started" to "null"
            is MobileDiagnosticEvent.WorkerFinished ->
                "worker.finished" to "{\"outcome\":\"${if (event.success) "success" else "failure"}\"}"
            is MobileDiagnosticEvent.PushReceived -> "push.received" to "null"
        }
        return "{\"schema_version\":1,\"seq\":0,\"wall_clock_ms\":$wallClockMs," +
            "\"elapsed_ms\":0,\"session_id\":\"test-session\",\"source\":\"android\"," +
            "\"cycle_id\":null,\"operation_id\":null,\"request_id\":null," +
            "\"event\":{\"name\":\"$eventName\",\"payload\":$payload}}"
    }

    private fun recorder(directory: File = dir): DiagnosticsRecorder = DiagnosticsRecorder(
        journalFn = { DiagnosticJournal(directory) },
        mintEventJsonFn = ::fixtureJson,
        nowMs = { 1_000L },
        elapsedRealtimeMs = { 0L },
    )

    private suspend fun waitUntilExported(rec: DiagnosticsRecorder, predicate: (String) -> Boolean) {
        var attempts = 0
        var exported = String(rec.export())
        while (!predicate(exported) && attempts < 100) {
            delay(20)
            exported = String(rec.export())
            attempts++
        }
        assertTrue("condition never became true; last export: $exported", predicate(exported))
    }

    @Test
    fun `record never blocks the caller, and the write eventually lands`() = runBlocking {
        val rec = recorder()

        // record() returns with no suspension point at all — calling it is
        // exactly as fast on the calling thread whether or not the
        // background write has happened yet.
        rec.record(MobileDiagnosticEvent.PushReceived)

        waitUntilExported(rec) { it.contains("push.received") }
    }

    @Test
    fun `export and clear round trip through the real journal`() = runBlocking {
        val rec = recorder()
        rec.record(MobileDiagnosticEvent.WorkerStarted)
        waitUntilExported(rec) { it.contains("worker.started") }

        rec.clear()

        assertEquals("""{"schema_version":1,"dropped_count":0,"events":[]}""", String(rec.export()))
    }

    @Test
    fun `a journal write failure is swallowed and record never throws`() {
        val blocker = File(dir, "blocked")
        blocker.writeText("a file, not a directory")
        val rec = DiagnosticsRecorder(
            journalFn = { DiagnosticJournal(File(blocker, "core")) },
            mintEventJsonFn = ::fixtureJson,
        )

        // No assertion beyond "this call returns" — a throw here would
        // fail the test on its own.
        rec.record(MobileDiagnosticEvent.PushReceived)
    }

    @Test
    fun `a failure minting the event is swallowed and record never throws`() {
        val rec = DiagnosticsRecorder(
            journalFn = { DiagnosticJournal(dir) },
            mintEventJsonFn = { _, _, _ -> throw IllegalStateException("boom") },
        )

        rec.record(MobileDiagnosticEvent.PushReceived)
    }

    @Test
    fun `a real exported fixture never carries a forbidden field`() = runBlocking {
        val rec = recorder()
        rec.record(MobileDiagnosticEvent.SessionStarted)
        rec.record(MobileDiagnosticEvent.WorkerStarted)
        rec.record(MobileDiagnosticEvent.WorkerFinished(success = false))
        rec.record(MobileDiagnosticEvent.PushReceived)
        waitUntilExported(rec) { it.contains("push.received") }

        val exported = String(rec.export()).lowercase()

        for (forbidden in FORBIDDEN_FIELD_KEYS) {
            assertFalse("exported diagnostics carried forbidden field: $forbidden", exported.contains(forbidden))
        }
        // And the export really did carry something worth checking —
        // otherwise the loop above would pass vacuously over an empty file.
        assertTrue(exported.contains("session.started"))
        assertTrue(exported.contains("worker.finished"))
    }

    companion object {
        /** Mirrors `hummingbird_core::diagnostics::FORBIDDEN_FIELD_NAMES`
         * (`client/core/src/diagnostics/mod.rs`), as exact-key JSON
         * substrings — the same redaction rule, checked here over what
         * Android's own export actually produces. */
        private val FORBIDDEN_FIELD_KEYS = listOf(
            "\"authorization\"", "\"access_token\"", "\"api_key\"", "\"token\"",
            "\"credential\"", "\"password\"", "\"body\"", "\"request_body\"",
            "\"response_body\"", "\"title\"", "\"description\"", "\"url\"",
            "\"ip\"", "\"ip_address\"", "\"exception\"", "\"stack_trace\"", "\"message\"",
        )
    }
}
