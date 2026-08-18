package net.twinion.hummingbird.skills

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import net.twinion.hummingbird.core.TokenStore
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.hummingbird_ffi_mobile.MobileGrillTurnState

// **#538's operator checkpoint.** The whole lane, against the real runner,
// on real hardware — the only place the probe's question ("can the phone
// drive the runner?") is actually answered.
//
// **This does not run in CI.** There is no emulator (#527's M1 traps), so
// `:app:testDebugUnitTest` + `:app:assembleDebug` are the automated
// evidence and this is the operator's. A green CI badge says nothing about
// anything in this file.
//
// **It spends real model tokens.** `grill-me` has no `apply` and writes
// nothing (ADR-0023), so unlike a `microtask` smoke test it leaves nothing
// behind — but the run is billed all the same.
//
// Running it (see `client/android/README.md` for the full procedure):
//
//     ./gradlew :app:connectedDebugAndroidTest \
//       -Pandroid.testInstrumentationRunnerArguments.ref=HB-42
//     adb logcat -s HB-SKILL-PROBE
//
// Every state transition is logged under one tag, so the evidence for the
// PR is that logcat output rather than a screenshot of a screen that does
// not exist yet — **this slice ships no UI**, which keeps #539 free to
// design the Grill takeover without inheriting a throwaway surface.
@RunWith(AndroidJUnit4::class)
class GrillTurnProbeTest {

    private val tag = "HB-SKILL-PROBE"

    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    /** The item to grill. An instrumentation argument, so nothing is
     * hardcoded to one item and the operator can point the probe at
     * whatever is actually foggy today. */
    private val ref: String by lazy {
        InstrumentationRegistry.getArguments().getString("ref")
            ?: error("pass -Pandroid.testInstrumentationRunnerArguments.ref=<item ref>")
    }

    private fun runner(): SkillRunner {
        // A named failure rather than a null-pointer three frames deeper:
        // the token is entered once in the app's Status screen, and
        // forgetting to is the likeliest way this run goes wrong.
        val token = TokenStore.load(context)
        check(!token.isNullOrEmpty()) {
            "no device token on this device — enter one in the app's Status screen first"
        }
        return SkillRunner.create(context)
    }

    private fun log(state: MobileGrillTurnState) {
        Log.i(tag, state.toString())
    }

    /**
     * Case 1: **a real turn, with heartbeats.** Opens the interview with an
     * empty `turns` list and runs it to a question or a proposal.
     *
     * The assertion that matters is the narration: the runner beats `"still
     * running"` every 20s, and a real turn takes long enough to see several
     * — the collapse is the core's reducer doing its job across a JNI
     * boundary on real timings, which is precisely what no unit test can
     * show.
     */
    @Test
    fun a_real_turn_answers_and_collapses_the_heartbeats() = runBlocking {
        val states = runner().grillTurn(ref, emptyList()).toList()
        states.forEach(::log)

        val last = states.last()
        assertTrue(
            "expected a question or a proposal, got $last",
            last is MobileGrillTurnState.Question || last is MobileGrillTurnState.Proposal,
        )
        val narration = when (last) {
            is MobileGrillTurnState.Question -> last.messages
            is MobileGrillTurnState.Proposal -> last.messages
            else -> emptyList()
        }
        Log.i(tag, "narration: $narration")
        assertTrue(
            "the heartbeat must collapse: no two consecutive entries may be equal — got $narration",
            narration.zipWithNext().none { (a, b) -> a == b },
        )
    }

    /**
     * Case 2: **a decline, carried verbatim.** An unresolvable `ref`
     * declines in the runner's `prepare`, before a model token is spent —
     * the cheap way to produce a real decline (threading past
     * `PROVISIONAL_TURN_CAP = 8` is the other, and costs eight turns).
     *
     * What is asserted is that the reason is the *runner's* words: nothing
     * on this side prefixes, rewords or branches on them.
     */
    @Test
    fun an_unresolvable_ref_declines_with_the_runners_own_words() = runBlocking {
        val states =
            runner().grillTurn("HB-000000-no-such-item", emptyList()).toList()
        states.forEach(::log)

        val last = states.last()
        assertTrue("expected a decline, got $last", last is MobileGrillTurnState.Declined)
        val declined = last as MobileGrillTurnState.Declined
        Log.i(tag, "decline reason: ${declined.reason} (answered=${declined.answered})")
        assertTrue("the decline must carry words", declined.reason.isNotEmpty())
        assertTrue(
            "a decline the runner itself answered must be marked answered",
            declined.answered,
        )
        // Not one of this client's own synthesized sentences: those are for
        // a socket that never got an answer, and this one did.
        assertFalse(
            "a runner decline must not be replaced by a transport decline",
            declined.reason.startsWith("Could not reach the server"),
        )
    }

    /**
     * Case 3: **cancellation mid-stream.** Stops collecting after the first
     * progress line; the `Call` is cancelled, the socket goes, and nothing
     * further is delivered. This is the one behaviour whose real cost is
     * invisible off-device — a run left streaming would keep the radio up
     * and keep spending.
     */
    @Test
    fun cancelling_mid_stream_delivers_nothing_further() = runBlocking {
        val states = runner().grillTurn(ref, emptyList()).take(2).toList()
        states.forEach(::log)

        assertEquals("nothing may be delivered after the collector stops", 2, states.size)
        assertTrue(
            "the turn must still have been in flight when it was cancelled — " +
                "if it had already settled this case proved nothing",
            states.last() is MobileGrillTurnState.Asking,
        )
    }
}
