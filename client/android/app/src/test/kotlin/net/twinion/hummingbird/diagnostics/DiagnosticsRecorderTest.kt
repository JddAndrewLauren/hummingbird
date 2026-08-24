package net.twinion.hummingbird.diagnostics

import java.io.File
import java.io.IOException
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
import uniffi.hummingbird_ffi_mobile.MobileWorkerTrigger

/** [DiagnosticsRecorder]'s own suite (#709). `mintEventJsonFn` is always a
 * fixture here, never the real `diagnosticEventJson` — that function is a
 * `#[uniffi::export]` call into the native `.so`, unreachable from a plain
 * JVM unit test (the same reason `SettingsViewModel` injects
 * `deadLetterHeadingFn`). The fixture's shape follows the same
 * `#[serde(tag = "name", content = "payload")]` rule `hummingbird-ffi-mobile`'s
 * own `diagnostics::tests` pins (a fieldless variant carries **no**
 * `payload` key, not `"payload":null` — review round 1 caught this file
 * getting that backwards), but **this fixture is not evidence about what
 * `diagnosticEventJson` actually produces** — nothing running in this
 * process can call that native function, so it cannot notice if the two
 * ever drift. The real redaction guarantee over real production output is
 * `hummingbird-ffi-mobile`'s own
 * `no_android_minted_event_ever_carries_a_forbidden_field` test (Rust);
 * what this class's own redaction-shaped test below checks is narrower —
 * only that the journal/export *pipeline* neither strips nor adds
 * anything to whatever JSON it is handed. */
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
        // A fieldless variant has no `payload` key at all — never
        // `"payload":null` — matching `#[serde(tag = "name", content =
        // "payload")]`'s real behavior for a unit variant.
        val eventField = when (event) {
            is MobileDiagnosticEvent.SessionStarted -> "{\"name\":\"session.started\"}"
            is MobileDiagnosticEvent.WorkerStarted ->
                "{\"name\":\"worker.started\",\"payload\":{\"trigger\":\"${event.trigger.name.lowercase()}\"," +
                    "\"attempt_count\":${event.attemptCount}}}"
            is MobileDiagnosticEvent.WorkerFinished ->
                "{\"name\":\"worker.finished\",\"payload\":{\"trigger\":\"${event.trigger.name.lowercase()}\"," +
                    "\"attempt_count\":${event.attemptCount}," +
                    "\"outcome\":\"${if (event.success) "success" else "failure"}\"}}"
            is MobileDiagnosticEvent.PushReceived -> "{\"name\":\"push.received\"}"
            is MobileDiagnosticEvent.NetworkChanged ->
                "{\"name\":\"network.changed\",\"payload\":{\"online\":${event.online}," +
                    "\"transport\":\"${event.transport.name.lowercase()}\"," +
                    "\"internet_capable\":${event.internetCapable},\"validated\":${event.validated}," +
                    "\"metered\":${event.metered},\"roaming\":${event.roaming}}}"
        }
        return "{\"schema_version\":1,\"seq\":0,\"wall_clock_ms\":$wallClockMs," +
            "\"elapsed_ms\":0,\"session_id\":\"test-session\",\"source\":\"android\"," +
            "\"cycle_id\":null,\"operation_id\":null,\"request_id\":null," +
            "\"event\":$eventField}"
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
        rec.record(MobileDiagnosticEvent.WorkerStarted(trigger = MobileWorkerTrigger.TIMER, attemptCount = 1u))
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
    fun `export swallows a journal read failure and answers the empty export shape`() = runBlocking {
        // `diagnostics.ndjson` as a *directory* rather than a file: `File
        // .readLines()` throws `IOException` reading it, giving `export`
        // a real failure to swallow rather than merely a Bash trick that
        // returns 0 without ever entering the exception path.
        File(dir, "diagnostics.ndjson").mkdirs()
        val rec = DiagnosticsRecorder(
            journalFn = { DiagnosticJournal(dir) },
            mintEventJsonFn = ::fixtureJson,
        )

        val exported = rec.export()

        assertEquals("""{"schema_version":1,"dropped_count":0,"events":[]}""", String(exported))
    }

    @Test
    fun `clear swallows a failure reaching the journal and never throws`() = runBlocking {
        // `File.delete()` cannot throw — it answers `false` — so a journal
        // whose files merely refuse to delete hands `clear`'s `runCatching`
        // nothing to catch, and the test this replaces passed just as well
        // with that `runCatching` deleted (review round 2 caught exactly
        // that vacuity). The failure `clear` genuinely has to survive is
        // one thrown on the way to, or inside, the journal — injected
        // through the same seam `a failure minting the event is swallowed`
        // already uses.
        val rec = DiagnosticsRecorder(
            journalFn = { throw IOException("no journal to clear") },
            mintEventJsonFn = ::fixtureJson,
        )

        // No assertion beyond "this call returns": `clear` `await()`s its
        // own coroutine, so an unswallowed throw surfaces right here.
        rec.clear()
    }

    /** The origin every `elapsed_ms` is measured from is sampled when the
     * recorder is *created* (`HummingbirdApp.onCreate`, i.e. process
     * start), never at the first `record()` — see
     * `DiagnosticsRecorder.Companion.create`. Both earlier forms failed
     * this: the first event reported `elapsed_ms: 0` however long the
     * process had already been running. */
    @Test
    fun `the origin is sampled at creation, so the first event's elapsed_ms is not zero`() = runBlocking {
        // A process that has already been up 4s when the recorder is made.
        var monotonicMs = 4_000L
        val originsPassedToRust = mutableListOf<Long>()
        val rec = DiagnosticsRecorder.create(
            directory = dir,
            elapsedRealtimeMs = { monotonicMs },
            initSessionFn = { _, origin -> originsPassedToRust += origin.toLong() },
            // Stands in for the native `diagnosticEventJson`, computing
            // `elapsed_ms` exactly as the Rust side does: the monotonic
            // reading at record time minus the session's fixed origin.
            eventJsonFn = { wallClockMs, monotonic, _ ->
                val origin = originsPassedToRust.last()
                """{"schema_version":1,"wall_clock_ms":$wallClockMs,""" +
                    """"elapsed_ms":${monotonic.toLong() - origin},"event":{"name":"push.received"}}"""
            },
        )

        // 5s of further process life before anything is recorded at all.
        monotonicMs = 9_000L
        rec.record(MobileDiagnosticEvent.PushReceived)
        waitUntilExported(rec) { it.contains("push.received") }

        // The origin handed to the Rust session is the creation-time
        // reading, not the record-time one — every time it is handed over.
        assertEquals(listOf(4_000L), originsPassedToRust.distinct())
        val exported = String(rec.export())
        assertTrue(
            "the first recorded event must report the real process uptime, not 0; export: $exported",
            exported.contains(""""elapsed_ms":5000"""),
        )
    }

    @Test
    fun `a failure minting the event is swallowed and record never throws`() {
        val rec = DiagnosticsRecorder(
            journalFn = { DiagnosticJournal(dir) },
            mintEventJsonFn = { _, _, _ -> throw IllegalStateException("boom") },
        )

        rec.record(MobileDiagnosticEvent.PushReceived)
    }

    /** Not the redaction guarantee itself (see this class's own doc) —
     * only that the export pipeline (append → rotate → export) copies
     * whatever JSON it is handed through verbatim, never wrapping,
     * stringifying or otherwise reintroducing a field the fixture never
     * had. The real guarantee, over real `diagnosticEventJson` output, is
     * `hummingbird-ffi-mobile`'s `no_android_minted_event_ever_carries_a_forbidden_field`. */
    @Test
    fun `the export pipeline never introduces a forbidden field of its own`() = runBlocking {
        val rec = recorder()
        rec.record(MobileDiagnosticEvent.SessionStarted)
        rec.record(MobileDiagnosticEvent.WorkerStarted(trigger = MobileWorkerTrigger.TIMER, attemptCount = 1u))
        rec.record(MobileDiagnosticEvent.WorkerFinished(trigger = MobileWorkerTrigger.TIMER, attemptCount = 1u, success = false))
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
        /** Mirrors `hummingbird_domain::diagnostics::FORBIDDEN_FIELD_NAMES`
         * (`server/domain/src/diagnostics.rs` — #711 moved the contract
         * there; `client/core/src/diagnostics/mod.rs` only re-exports the
         * public items and holds no list), as exact-key JSON substrings —
         * the same redaction rule, checked here over what Android's own
         * export actually produces. Nothing gates the two against drift
         * (#741), so keep this pointer exact. */
        private val FORBIDDEN_FIELD_KEYS = listOf(
            "\"authorization\"", "\"access_token\"", "\"api_key\"", "\"token\"",
            "\"credential\"", "\"password\"", "\"body\"", "\"request_body\"",
            "\"response_body\"", "\"title\"", "\"description\"", "\"url\"",
            "\"ip\"", "\"ip_address\"", "\"exception\"", "\"stack_trace\"", "\"message\"",
        )
    }
}
