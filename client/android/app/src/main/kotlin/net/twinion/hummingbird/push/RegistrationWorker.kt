package net.twinion.hummingbird.push

import android.content.Context
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters

// Push registration's background leg (M2/#141). Registration is a direct
// `POST /api/push_targets` rather than a queued mutation — push targets
// carry no `version` and never delta-pull, so there is nothing for a queue
// entry to rebase against (`MobileTaskHost.registerPushTarget`'s doc) —
// which means it needs the network *now* and a retry of its own when the
// network is not there. That is exactly a constrained WorkManager job.
//
// The decision itself lives in [RegistrationRunner]; this class is only the
// WorkManager shell around it.
class RegistrationWorker(context: Context, params: WorkerParameters) :
    CoroutineWorker(context, params) {

    override suspend fun doWork(): Result =
        when (RegistrationRunner.create(applicationContext).run()) {
            RegistrationOutcome.REGISTERED,
            RegistrationOutcome.NO_TOKEN,
            RegistrationOutcome.UNAUTHORIZED,
            -> Result.success()
            RegistrationOutcome.RETRY -> Result.retry()
        }

    companion object {
        /** One in-flight registration at a time, and the newest wins.
         *
         * `REPLACE`, not `KEEP`: the two events that enqueue this work are
         * "FCM rotated the token" and "a device token arrived", and in both
         * cases an already-queued attempt is working from stale inputs. The
         * unique name is what keeps a rotation storm from stacking jobs. */
        const val UNIQUE_WORK = "push-register"

        fun enqueue(context: Context) {
            val request = OneTimeWorkRequestBuilder<RegistrationWorker>()
                .setConstraints(
                    Constraints.Builder()
                        .setRequiredNetworkType(NetworkType.CONNECTED)
                        .build(),
                )
                .build()
            WorkManager.getInstance(context.applicationContext).enqueueUniqueWork(
                UNIQUE_WORK,
                ExistingWorkPolicy.REPLACE,
                request,
            )
        }
    }
}
