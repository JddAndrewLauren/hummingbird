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
        val core = CoreHolder.get(applicationContext)
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
        return when (outcome.kind) {
            "pull_failed", "persist_failed", "blocked" -> Result.retry()
            else -> Result.success()
        }
    }

    companion object {
        const val KEY_TRIGGER = "trigger"

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
    }
}
