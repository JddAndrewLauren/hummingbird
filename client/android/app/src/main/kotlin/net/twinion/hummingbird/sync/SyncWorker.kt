package net.twinion.hummingbird.sync

import android.content.Context
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters
import kotlin.random.Random
import net.twinion.hummingbird.core.CoreHolder

// The hourly background leg of the sync model decided on #141 (grilling
// 2026-08-14): foreground sync + an OS-deferrable ~hourly WorkManager
// refresh + sync-on-push (the FCM trigger arrives with M2). This worker is
// the middle leg only — it keeps the mirror warm-ish while the app is
// closed, and nothing about correctness depends on it running.
class SyncWorker(context: Context, params: WorkerParameters) :
    CoroutineWorker(context, params) {

    override suspend fun doWork(): Result {
        val core = CoreHolder.get(applicationContext)
        val outcome = core.run(
            System.currentTimeMillis(),
            // "timer": gated by the core's backoff, exactly the web
            // client's cadence tick — a background refresh is not a
            // deliberate user gesture.
            "timer",
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
}
