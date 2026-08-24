package net.twinion.hummingbird.sync

import android.content.Context
import androidx.work.CoroutineWorker
import androidx.work.Data
import androidx.work.OneTimeWorkRequest
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.WorkerParameters
import kotlin.random.Random
import net.twinion.hummingbird.core.CoreHolder
import net.twinion.hummingbird.diagnostics.DiagnosticsRecorder
import uniffi.hummingbird_ffi_mobile.MobileDiagnosticEvent
import uniffi.hummingbird_ffi_mobile.MobileWorkerTrigger

// The background legs of the sync model decided on #141 (grilling
// 2026-08-14): foreground sync + an OS-deferrable ~hourly WorkManager
// refresh + sync-on-push. This worker is both background legs — the hourly
// one it always was, and (M2) the one-shot a push enqueues.
//
// **Still no competing clock.** The hourly cadence has exactly one owner
// (`HummingbirdApp.scheduleHourlySync`, `KEEP`). The push leg is not a
// second cadence: it is event-driven, one run per arriving message, and it
// schedules nothing. The rule bans a second thing *ticking*, not a second
// caller.
//
// The trigger is an input rather than a constant now, and which one is sent
// is load-bearing. `ffi-mobile` maps any non-`"timer"` string to
// `Trigger::User`, which is *not* backoff-gated — and the ack path needs
// exactly that: an ack of an alert this device has not synced must be able
// to fetch the row right now, even if a recent failure left the core
// backing off, or the ack raises an `AlertNotFound` that no retry resolves.
class SyncWorker(context: Context, params: WorkerParameters) :
    CoroutineWorker(context, params) {

    override suspend fun doWork(): Result {
        val recorder = DiagnosticsRecorder.get(applicationContext)
        // #710: `runAttemptCount` is only readable inside the worker itself
        // — it is what tells a first failure apart from a backoff loop, so
        // both `worker.started` and `worker.finished` carry it, alongside
        // the trigger that started this run.
        val trigger = triggerOf(inputData.getString(KEY_TRIGGER))
        recorder.record(MobileDiagnosticEvent.WorkerStarted(trigger = trigger, attemptCount = runAttemptCount.toUInt()))
        val core = CoreHolder.get(applicationContext)
        // #710 review round 1: drained here too, *before* the call that can
        // hang — `takeDiagnosticEvents` never touches the mutex `run`
        // below acquires, so this cannot block on it, and it means a
        // never-returning `run` (#704's own scenario) does not strand
        // whatever `core.wait_started`/`core.acquired`/`core.released`
        // spans a concurrent capture/triage/read already buffered before
        // this run started. The drain after `run` (below) still catches
        // this run's own spans on every ordinary, non-hung completion.
        core.takeDiagnosticEvents().forEach { line -> recorder.appendRaw(line.json, line.wallClockMs) }
        val outcome = core.run(
            System.currentTimeMillis(),
            inputData.getString(KEY_TRIGGER) ?: TRIGGER_TIMER,
            false,
            Random.nextDouble(),
        )
        // Retryable outcomes ride WorkManager's own backoff; everything
        // else (completed, skipped, no_credential, held) is this run done.
        // The core has already recorded its own backoff either way — a
        // Retry here only re-offers the attempt, it cannot bypass that.
        val retryable = isRetryable(outcome.kind)
        recorder.record(
            MobileDiagnosticEvent.WorkerFinished(
                trigger = trigger,
                attemptCount = runAttemptCount.toUInt(),
                success = !retryable,
            ),
        )
        // #710: drains the mobile FFI host's own buffered `core.*`/
        // `operation.*` spans (`core_lock`'s module doc) into the real
        // journal — the second of this run's two drains, the one that
        // catches this run's own spans. The other drain sites are the
        // pre-`run` drain above and `SettingsViewModel`'s export path,
        // which flushes the same buffer before writing the export. Each
        // drain is a raw pre-minted line: `appendRaw` skips
        // `mintEventJsonFn` entirely, it never re-serializes what Rust
        // already built.
        core.takeDiagnosticEvents().forEach { line -> recorder.appendRaw(line.json, line.wallClockMs) }
        return if (retryable) Result.retry() else Result.success()
    }

    companion object {
        const val KEY_TRIGGER = "trigger"

        /** The outcomes WorkManager retries (its own backoff, above) —
         * everything else counts as `worker.finished { success: true }`
         * for #709's diagnostic event, even "skipped"/"no_credential"/
         * "held": none of those is a *worker* failure, they are the core
         * declining to do work this run. */
        private val RETRYABLE_OUTCOME_KINDS = setOf("pull_failed", "persist_failed", "blocked")

        /** [RETRYABLE_OUTCOME_KINDS]'s own predicate, exposed for
         * `SyncWorkerTest` — the mapping is the load-bearing logic behind
         * both the returned [Result] and `worker.finished`'s `success`
         * field, so it is worth pinning directly rather than only through
         * a full `doWork()` run (which would need a live authority to
         * reach a real "credential_needed" 401 outcome). `"credential_needed"`
         * — the 401 case #710's brief names explicitly — answers `false`
         * here: it is the core declining to work, not a worker failure. */
        internal fun isRetryable(outcomeKind: String): Boolean = outcomeKind in RETRYABLE_OUTCOME_KINDS

        /** The periodic leg's trigger: gated by the core's backoff,
         * exactly the web client's cadence tick — a background refresh is
         * not a deliberate user gesture. The default when none is given,
         * so the hourly work needs no input data. */
        const val TRIGGER_TIMER = "timer"

        /** The push leg's trigger. Any non-`"timer"` string reaches the
         * core as `Trigger::User` and so bypasses backoff gating
         * (`ffi-mobile/src/lib.rs`); a push is evidence the mirror is
         * stale *now*, and the ack that may follow needs the row. */
        const val TRIGGER_PUSH = "push"

        /** The one-shot a received push enqueues.
         *
         * `RUN_AS_NON_EXPEDITED_WORK_REQUEST` is mandatory, not a
         * preference: without an out-of-quota policy, enqueueing expedited
         * work **throws** once the app's foreground-service quota is
         * exhausted, and a throw inside `onMessageReceived` loses the sync
         * entirely. Degrading to ordinary work is the correct fallback —
         * later is fine, never is not. */
        fun expeditedOnPush(): OneTimeWorkRequest =
            OneTimeWorkRequestBuilder<SyncWorker>()
                .setInputData(Data.Builder().putString(KEY_TRIGGER, TRIGGER_PUSH).build())
                .setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
                .build()

        /** [KEY_TRIGGER]'s raw string, resolved to #710's closed
         * `worker.started`/`worker.finished` vocabulary — any value other
         * than [TRIGGER_PUSH] reads as [MobileWorkerTrigger.TIMER], the
         * same "default to timer" the periodic leg's own lack of input
         * data already relies on. */
        internal fun triggerOf(trigger: String?): MobileWorkerTrigger =
            if (trigger == TRIGGER_PUSH) MobileWorkerTrigger.PUSH else MobileWorkerTrigger.TIMER
    }
}
