package net.twinion.hummingbird.diagnostics

import android.content.Context
import android.os.SystemClock
import java.io.File
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.hummingbird_ffi_mobile.MobileDiagnosticEvent
import uniffi.hummingbird_ffi_mobile.diagnosticEventJson
import uniffi.hummingbird_ffi_mobile.diagnosticInitSession

/**
 * The one process-wide diagnostic recorder (#709). Reachable from
 * foreground UI (`SettingsScreen`'s Export/Clear), `SyncWorker` (around
 * `MobileTaskHost.run`), push handling (`HbMessagingService
 * .onMessageReceived`) and the mobile FFI host (`CoreHolder`, right after
 * `MobileTaskHost.init`) — [Companion.get] hands every one of those four
 * call sites the same instance. No second recorder or per-component
 * journal exists anywhere in this app.
 *
 * **Never blocks a caller, whatever thread or coroutine scope it is
 * called from.** [record] mints the event's JSON synchronously (cheap: no
 * I/O, just a JNI call and a `String`) and then *enqueues* the file write
 * onto [scope] — a `SupervisorJob` on a single-threaded IO dispatcher, tied
 * to this object's own lifetime rather than to any caller's, so an
 * `HbMessagingService` callback that must return quickly, or a `SyncWorker`
 * coroutine that gets cancelled, never waits on or interrupts a diagnostic
 * write. Serializing every write onto one dispatcher is also what keeps
 * two concurrent callers from ever interleaving a partial line.
 *
 * **Never disturbs its caller, however the write itself goes.** [record]
 * swallows any failure minting or appending the event — the brief's own
 * "a diagnostic write failure (no space, IO error, serialization failure)
 * is swallowed" criterion — because nothing that merely observes the app
 * (capture, a sync cycle, startup) may behave differently depending on
 * whether this recorder happens to be working. [export]/[clear] carry the
 * same swallowing: a failure reading or deleting the journal's files
 * answers an empty export (or a no-op clear) rather than throwing out of
 * `scope.launch`/`.await()` into a caller with no handler for it — Settings'
 * own coroutine included, where an uncaught exception there would crash
 * the app rather than merely fail to export.
 *
 * **[export]/[clear] resist the `NavBackStackEntry` cancellation trap.**
 * Settings' Export/Clear buttons call these from a `ViewModel`-scoped
 * coroutine, and that scope is cancelled the moment its screen pops — so
 * each is enqueued onto [scope] (never a child of the caller's own scope)
 * and merely `await()`-ed: a caller that gets cancelled mid-write only
 * cancels its own `await`, not the write already under way, which keeps
 * running under [NonCancellable] until the journal is actually consistent.
 */
class DiagnosticsRecorder(
    private val journalFn: () -> DiagnosticJournal,
    /** Mints one event's NDJSON line — `diagnosticInitSession` +
     * `diagnosticEventJson` in production, and (per the house rule
     * `SettingsViewModel.deadLetterHeadingFn` already states) an injected
     * fake in every plain-JVM unit test, so those tests never touch the
     * native `.so` at all. */
    private val mintEventJsonFn: (event: MobileDiagnosticEvent, wallClockMs: Long, monotonicMs: Long) -> String,
    private val nowMs: () -> Long = System::currentTimeMillis,
    private val elapsedRealtimeMs: () -> Long = SystemClock::elapsedRealtime,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO.limitedParallelism(1))

    fun record(event: MobileDiagnosticEvent) {
        val wallClockMs = nowMs()
        val json = runCatching { mintEventJsonFn(event, wallClockMs, elapsedRealtimeMs()) }
            .getOrNull() ?: return
        scope.launch {
            withContext(NonCancellable) {
                runCatching { journalFn().append(json, wallClockMs) }
            }
        }
    }

    suspend fun export(): ByteArray {
        val result = scope.async {
            withContext(NonCancellable) { runCatching { journalFn().export() } }
        }.await()
        return result.getOrDefault(EMPTY_EXPORT)
    }

    suspend fun clear() {
        scope.async {
            withContext(NonCancellable) { runCatching { journalFn().clear() } }
        }.await()
    }

    companion object {
        /** [export]'s fallback when reading the journal fails — the exact
         * bytes an empty, healthy [DiagnosticJournal] itself would produce
         * (its own `export()` doc), so a caller cannot tell "the journal
         * failed to read" from "the journal is empty" — both answer with
         * nothing to show, which is the whole point of swallowing this. */
        private val EMPTY_EXPORT: ByteArray =
            """{"schema_version":1,"dropped_count":0,"events":[]}""".toByteArray(Charsets.UTF_8)

        @Volatile
        private var instance: DiagnosticsRecorder? = null

        /** Minted once per process, the first time this singleton is
         * created — the one monotonic reading every event's `elapsed_ms`
         * in this process is measured from
         * (`diagnosticInitSession`'s own contract on the Rust side). */
        private val sessionId: String by lazy { UUID.randomUUID().toString() }
        private val originMonotonicMs: Long by lazy { SystemClock.elapsedRealtime() }

        /** The app-private core directory (`client/core/src/storage/fs.rs`'s
         * own name for it) — the same directory `CoreHolder` already
         * namespaces the core's snapshot store into, reused rather than a
         * second location this app would have to invent. */
        private fun coreDirectory(context: Context): File =
            File(context.applicationContext.filesDir, "hummingbird-core")

        fun get(context: Context): DiagnosticsRecorder {
            instance?.let { return it }
            synchronized(this) {
                instance?.let { return it }
                val directory = coreDirectory(context)
                val created = DiagnosticsRecorder(
                    journalFn = { DiagnosticJournal(directory) },
                    mintEventJsonFn = { event, wallClockMs, monotonicMs ->
                        diagnosticInitSession(sessionId, originMonotonicMs.toULong())
                        diagnosticEventJson(wallClockMs, monotonicMs.toULong(), event)
                    },
                )
                instance = created
                return created
            }
        }
    }
}
