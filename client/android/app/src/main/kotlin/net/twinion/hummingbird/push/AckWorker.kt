package net.twinion.hummingbird.push

import android.content.Context
import androidx.work.CoroutineWorker
import androidx.work.Data
import androidx.work.OneTimeWorkRequest
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.WorkerParameters
import net.twinion.hummingbird.notify.AlertNotifier

// The Ack action's background half (M2/#141). [AckReceiver] gets ~10
// seconds; a sync-then-CAS retry can want more than that, so the receiver
// enqueues this and returns.
class AckWorker(context: Context, params: WorkerParameters) :
    CoroutineWorker(context, params) {

    override suspend fun doWork(): Result {
        val alertId = inputData.getString(KEY_ALERT_ID) ?: return Result.success()
        // A capped retry, not an open-ended one. `AckFailed` means the
        // authority was unreachable or refused; three attempts covers a
        // blip, and past that the alerts surface still offers the Ack
        // button on a row that is still `canAck` — a durable place for the
        // gesture to be retried by the person, which a silent forever-loop
        // is not.
        if (runAttemptCount >= MAX_ATTEMPTS) return Result.failure()
        return when (AckRunner.create(applicationContext).run(alertId)) {
            AckOutcome.DONE -> {
                // The ring should not outlive the thing it was about.
                AlertNotifier.cancel(applicationContext, alertId)
                Result.success()
            }
            AckOutcome.RETRY -> Result.retry()
        }
    }

    companion object {
        const val KEY_ALERT_ID = "alert_id"
        private const val MAX_ATTEMPTS = 3

        /** The one-shot [AckReceiver] enqueues. Expedited, because this is
         * a gesture a person just made and is watching for; the
         * out-of-quota fallback is mandatory for the same reason it is on
         * `SyncWorker.expeditedOnPush` — enqueueing expedited work throws
         * once quota is gone, and a throw here drops the ack. */
        fun expedited(alertId: String): OneTimeWorkRequest =
            OneTimeWorkRequestBuilder<AckWorker>()
                .setInputData(Data.Builder().putString(KEY_ALERT_ID, alertId).build())
                .setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
                .build()
    }
}
