package net.twinion.hummingbird.sync

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileDiagnosticEvent
import uniffi.hummingbird_ffi_mobile.MobileWorkerTrigger

/**
 * [SyncWorker]'s own diagnostic surface (#710): `triggerOf`/`isRetryable`
 * are the whole load-bearing logic behind `worker.started`/
 * `worker.finished`'s payload and the [androidx.work.ListenableWorker.Result]
 * this worker returns, so they are pinned directly as plain functions here.
 *
 * **What this suite deliberately does not do.** A full `doWork()` run
 * reaches `CoreHolder.get(applicationContext)` — a process-wide singleton
 * with no injection seam — and from there a real `MobileTaskHost.run`
 * against a real authority origin. Neither `runAttemptCount` (a
 * `WorkerParameters` field WorkManager itself guarantees, with no logic of
 * this app's own to test) nor a live 401 round trip is reachable from a
 * hermetic JVM unit test without either a running authority or a
 * substantial `SyncWorker` refactor to make its core host injectable —
 * out of scope for this slice (a residual gap, named in this issue's own
 * report). What *is* tested here is everything this worker itself decides:
 * which trigger string maps to which [MobileWorkerTrigger], and which
 * outcome kind — including `"credential_needed"`, the 401 case — is
 * retryable.
 */
class SyncWorkerTest {

    @Test
    fun `the push trigger string maps to PUSH, everything else to TIMER`() {
        assertEquals(MobileWorkerTrigger.PUSH, SyncWorker.triggerOf(SyncWorker.TRIGGER_PUSH))
        assertEquals(MobileWorkerTrigger.TIMER, SyncWorker.triggerOf(SyncWorker.TRIGGER_TIMER))
        assertEquals(MobileWorkerTrigger.TIMER, SyncWorker.triggerOf(null))
        assertEquals(MobileWorkerTrigger.TIMER, SyncWorker.triggerOf("some-unrecognised-string"))
    }

    @Test
    fun `pull_failed persist_failed and blocked are retryable`() {
        assertTrue(SyncWorker.isRetryable("pull_failed"))
        assertTrue(SyncWorker.isRetryable("persist_failed"))
        assertTrue(SyncWorker.isRetryable("blocked"))
    }

    /** The 401 case #710's brief names explicitly: a stale/rejected token
     * surfaces as `CycleOutcome::CredentialNeeded` → `"credential_needed"`
     * on the mobile seam (`ffi-mobile::map_run_outcome`) — the core
     * declining to work because it has nothing to reach the authority
     * with, not a worker failure WorkManager should re-attempt on its own
     * backoff. */
    @Test
    fun `credential_needed the 401 case is not retryable`() {
        assertFalse(SyncWorker.isRetryable("credential_needed"))
    }

    @Test
    fun `completed and skipped are not retryable`() {
        assertFalse(SyncWorker.isRetryable("completed"))
        assertFalse(SyncWorker.isRetryable("skipped"))
        assertFalse(SyncWorker.isRetryable("no_credential"))
        assertFalse(SyncWorker.isRetryable("held"))
    }

    /** The events a WorkManager retry produces, end to end through the
     * real [MobileDiagnosticEvent] constructors — no token field exists on
     * either shape for a value to leak through in the first place, which
     * is what "with no token value recorded" reduces to for a type this
     * closed: there is nowhere on [MobileDiagnosticEvent.WorkerStarted] or
     * [MobileDiagnosticEvent.WorkerFinished] a token could ride. */
    @Test
    fun `a retryable run's events carry the trigger and attempt count, never a token`() {
        val trigger = MobileWorkerTrigger.TIMER
        val attemptCount = 3u
        val started = MobileDiagnosticEvent.WorkerStarted(trigger = trigger, attemptCount = attemptCount)
        val outcomeKind = "persist_failed"
        val finished = MobileDiagnosticEvent.WorkerFinished(
            trigger = trigger,
            attemptCount = attemptCount,
            success = !SyncWorker.isRetryable(outcomeKind),
        )

        assertEquals(trigger, started.trigger)
        assertEquals(attemptCount, started.attemptCount)
        assertFalse(finished.success)
        assertEquals(attemptCount, finished.attemptCount)
        assertFalse("no token field exists on this shape", started.toString().lowercase().contains("token"))
        assertFalse("no token field exists on this shape", finished.toString().lowercase().contains("token"))
    }

    /** The 401/`credential_needed` case's own events — `success: true`,
     * since the core declining to sync for lack of a credential is not a
     * worker failure (this file's own `isRetryable` test above). */
    @Test
    fun `a credential_needed run's finished event reports success and no token`() {
        val trigger = MobileWorkerTrigger.PUSH
        val finished = MobileDiagnosticEvent.WorkerFinished(
            trigger = trigger,
            attemptCount = 1u,
            success = !SyncWorker.isRetryable("credential_needed"),
        )

        assertTrue(finished.success)
        assertFalse(finished.toString().lowercase().contains("token"))
    }
}
