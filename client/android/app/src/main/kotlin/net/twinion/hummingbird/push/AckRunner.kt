package net.twinion.hummingbird.push

import android.content.Context
import kotlin.random.Random
import net.twinion.hummingbird.core.CoreHolder
import uniffi.hummingbird_ffi_mobile.MobileAlertException

/** What one ack attempt decided — [AckWorker]'s whole output, kept free of
 * `androidx.work` so the decision is testable on a plain JVM. */
enum class AckOutcome {
    /** Acked, or settled in a way no retry improves. Cancel the
     * notification and stop. */
    DONE,

    /** A transport-level failure; try again on WorkManager's backoff. */
    RETRY,
}

// The Ack action's decision (M2/#141, ADR-0012), lifted out of [AckWorker]
// the way [RegistrationRunner] is lifted out of its own worker.
//
// The whole reason this is not a one-liner is the deep-link race. The push
// payload carries no version — acking is sync-then-CAS against the mirror
// (`MobileTaskHost.ackAlert`'s doc) — and the push can beat the sync that
// would have brought the row down. `ackAlert` then raises `AlertNotFound`
// rather than inventing a version, and the host's answer is prescribed:
// run a cycle, retry once. Never guess a version; a wrong `expected_version`
// either loses a concurrent change or fails a CAS that then looks like a
// transport error.
//
// Still not found *after a cycle that actually completed* is not an error
// worth retrying. It means the authority does not have that alert for this
// device — retired, expired, or belonging to a slot this install no longer
// holds — and a backoff cannot change that. The user's gesture is honoured
// by retiring the notification, not by looping.
//
// **Which is why the sync leg reports whether it ran.** `MobileTaskHost.run`
// *returns* its failure (`pull_failed`, `blocked`, `no_credential`, …)
// rather than throwing, so a discarded return value turns "the device is
// offline" into the same silence as "the row is gone": the second
// `AlertNotFound` would read as authoritative, [AckWorker] would cancel the
// notification, and the ack the person made would exist nowhere — no CAS
// queued, no ring left to try again from. Offline is the *common* case for
// a lock-screen Ack, so [syncFn] answers `true` only for a completed cycle
// and anything else retries on WorkManager's backoff.
class AckRunner(
    private val ackFn: suspend (alertId: String) -> Unit,
    /** Runs one cycle; `true` iff it completed, i.e. the mirror is now as
     * current as the authority could make it. */
    private val syncFn: suspend () -> Boolean,
) {

    suspend fun run(alertId: String): AckOutcome =
        try {
            ackFn(alertId)
            AckOutcome.DONE
        } catch (_: MobileAlertException.AlertNotFound) {
            val synced = syncFn()
            try {
                ackFn(alertId)
                AckOutcome.DONE
            } catch (_: MobileAlertException.AlertNotFound) {
                if (synced) AckOutcome.DONE else AckOutcome.RETRY
            } catch (_: MobileAlertException.AckFailed) {
                AckOutcome.RETRY
            }
        } catch (_: MobileAlertException.AckFailed) {
            AckOutcome.RETRY
        }

    companion object {
        /** The production wiring, over the app's one durable core handle.
         *
         * The sync leg is a direct `run` with the `"push"` trigger rather
         * than an enqueued [net.twinion.hummingbird.sync.SyncWorker]: this
         * runs *inside* a worker already, and the retry below has to see
         * the row the cycle brought down. A non-`"timer"` trigger reaches
         * the core as `Trigger::User` and so is not backoff-gated — which
         * matters most in exactly the case that got here, where something
         * has recently gone wrong. */
        fun create(context: Context): AckRunner {
            val app = context.applicationContext
            return AckRunner(
                ackFn = { alertId ->
                    CoreHolder.get(app).ackAlert(alertId, System.currentTimeMillis())
                },
                syncFn = {
                    // `completed` and nothing else: `skipped` and `held`
                    // mean this cycle pulled nothing, which is exactly as
                    // uninformative about the missing row as a transport
                    // failure is.
                    CoreHolder.get(app).run(
                        System.currentTimeMillis(),
                        net.twinion.hummingbird.sync.SyncWorker.TRIGGER_PUSH,
                        false,
                        Random.nextDouble(),
                    ).kind == "completed"
                },
            )
        }
    }
}
