package net.twinion.hummingbird.push

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileAlertException

// [AckRunner]'s control flow with fakes only. What is under test is the
// deep-link race: the push carries no version, so acking is sync-then-CAS
// against the mirror, and the push can beat the sync that would have
// brought the row down. `ackAlert` refuses (`AlertNotFound`) rather than
// inventing a version; the ordering below is the host's prescribed answer.
class AckRunnerTest {

    /** Records the order of the two legs, so "synced *then* retried" is an
     * assertion rather than an inference from call counts. */
    private class Trace {
        val calls = mutableListOf<String>()
    }

    @Test
    fun `a first-try ack is done, with no sync at all`() = runBlocking {
        val trace = Trace()
        val runner = AckRunner(
            ackFn = { trace.calls += "ack" },
            syncFn = { trace.calls += "sync"; true },
        )

        assertEquals(AckOutcome.DONE, runner.run("alert-1"))
        assertEquals(listOf("ack"), trace.calls)
    }

    @Test
    fun `not-found syncs once and retries the ack, in that order`() = runBlocking {
        val trace = Trace()
        var attempts = 0
        val runner = AckRunner(
            ackFn = {
                trace.calls += "ack"
                if (attempts++ == 0) throw MobileAlertException.AlertNotFound()
            },
            syncFn = { trace.calls += "sync"; true },
        )

        assertEquals(AckOutcome.DONE, runner.run("alert-1"))
        assertEquals(listOf("ack", "sync", "ack"), trace.calls)
    }

    @Test
    fun `still not found after a completed sync gives up rather than looping`() = runBlocking {
        val trace = Trace()
        val runner = AckRunner(
            ackFn = {
                trace.calls += "ack"
                throw MobileAlertException.AlertNotFound()
            },
            syncFn = { trace.calls += "sync"; true },
        )

        // The authority does not have this alert for this device. A
        // backoff cannot change that, so the gesture is honoured by
        // retiring the notification, not by retrying forever.
        assertEquals(AckOutcome.DONE, runner.run("alert-1"))
        assertEquals(listOf("ack", "sync", "ack"), trace.calls)
        assertTrue("exactly one sync, never a loop", trace.calls.count { it == "sync" } == 1)
    }

    @Test
    fun `AckFailed retries and never syncs -- the row is there, the network was not`() = runBlocking {
        val trace = Trace()
        val runner = AckRunner(
            ackFn = {
                trace.calls += "ack"
                throw MobileAlertException.AckFailed("timeout")
            },
            syncFn = { trace.calls += "sync"; true },
        )

        assertEquals(AckOutcome.RETRY, runner.run("alert-1"))
        assertEquals(listOf("ack"), trace.calls)
    }

    @Test
    fun `a transport failure on the post-sync retry still retries`() = runBlocking {
        var attempts = 0
        val runner = AckRunner(
            ackFn = {
                if (attempts++ == 0) throw MobileAlertException.AlertNotFound()
                throw MobileAlertException.AckFailed("503")
            },
            syncFn = { true },
        )

        assertEquals(AckOutcome.RETRY, runner.run("alert-1"))
    }

    @Test
    fun `a sync that did not complete makes not-found inconclusive, so it retries`() = runBlocking {
        val trace = Trace()
        val runner = AckRunner(
            ackFn = {
                trace.calls += "ack"
                throw MobileAlertException.AlertNotFound()
            },
            // `run` reports `pull_failed`/`blocked`/`no_credential` by
            // returning, never by throwing. The row may well exist; this
            // device simply never got to look. Retiring the notification
            // here would drop the person's ack with nothing queued.
            syncFn = { trace.calls += "sync"; false },
        )

        assertEquals(AckOutcome.RETRY, runner.run("alert-1"))
        assertEquals(listOf("ack", "sync", "ack"), trace.calls)
    }

    @Test
    fun `the alert id reaches the ack fn unchanged`() = runBlocking {
        var seen: String? = null
        AckRunner(ackFn = { seen = it }, syncFn = { true }).run("alert-xyz")

        assertEquals("alert-xyz", seen)
    }
}
