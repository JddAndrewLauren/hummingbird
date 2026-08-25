package net.twinion.hummingbird.push

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobilePushRegistrationException

// [RegistrationRunner]'s control flow with fakes only — no WorkManager, no
// Firebase, no generated JNI binding (`NowViewModelTest`'s own reasoning).
// What is under test is the retry asymmetry: the two failure modes of
// `registerPushTarget` mean opposite things about whether waiting helps.
class RegistrationRunnerTest {

    private fun runner(
        installId: String = "slot-1",
        fcmToken: String? = "fcm-abc",
        register: suspend (String, String, String) -> Unit = { _, _, _ -> },
        logFn: (String) -> Unit = {},
    ) = RegistrationRunner(
        installIdFn = { installId },
        fcmTokenFn = { fcmToken },
        registerFn = register,
        logFn = logFn,
    )

    @Test
    fun `a successful registration is REGISTERED`() = runBlocking {
        assertEquals(RegistrationOutcome.REGISTERED, runner().run())
    }

    @Test
    fun `no cached FCM token is NO_TOKEN and does not retry`() = runBlocking {
        var called = false
        val outcome = runner(fcmToken = null, register = { _, _, _ -> called = true }).run()

        assertEquals(RegistrationOutcome.NO_TOKEN, outcome)
        assertTrue("nothing to register means nothing is sent", !called)
    }

    @Test
    fun `Unauthorized is UNAUTHORIZED and does not retry -- only a pasted device token changes it`() =
        runBlocking {
            val outcome = runner(
                register = { _, _, _ -> throw MobilePushRegistrationException.Unauthorized() },
            ).run()

            // A backoff cannot produce a credential. `MainActivity`'s
            // onSaveToken re-enqueues this work when one arrives.
            assertEquals(RegistrationOutcome.UNAUTHORIZED, outcome)
        }

    @Test
    fun `RegisterFailed retries -- registration is idempotent by slot id`() = runBlocking {
        val outcome = runner(
            register = { _, _, _ ->
                throw MobilePushRegistrationException.RegisterFailed("503 upstream")
            },
        ).run()

        assertEquals(RegistrationOutcome.RETRY, outcome)
    }

    @Test
    fun `every attempt sends the same install id -- a fresh id would strand a slot`() = runBlocking {
        val sentIds = mutableListOf<String>()
        val runner = runner(register = { id, _, _ -> sentIds += id })

        runner.run()
        runner.run()

        assertEquals(listOf("slot-1", "slot-1"), sentIds)
    }

    @Test
    fun `the cached FCM token is what gets registered`() = runBlocking {
        var sentToken: String? = null
        runner(fcmToken = "fcm-rotated", register = { _, _, token -> sentToken = token }).run()

        assertEquals("fcm-rotated", sentToken)
    }

    // #519: the outcome alone tells the caller what to do next
    // (`RegistrationWorker` maps it to `Result`), but a human reading logcat
    // needs to tell the four cases apart too -- otherwise DONE-shaped
    // outcomes read identically and the actual case is a process of
    // elimination. Each case's line is asserted distinct from the others.
    @Test
    fun `each outcome emits its own distinguishable log line`() = runBlocking {
        val lines = mutableListOf<String>()
        val log: (String) -> Unit = { lines += it }

        runner(logFn = log).run()
        runner(fcmToken = null, logFn = log).run()
        runner(
            logFn = log,
            register = { _, _, _ -> throw MobilePushRegistrationException.Unauthorized() },
        ).run()
        runner(
            logFn = log,
            register = { _, _, _ -> throw MobilePushRegistrationException.RegisterFailed("503") },
        ).run()

        assertEquals("one line per attempt", 4, lines.size)
        assertEquals("every line is distinct", 4, lines.toSet().size)
    }
}
