package net.twinion.hummingbird.skills

import android.content.Context
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.launch
import net.twinion.hummingbird.BuildConfig
import net.twinion.hummingbird.core.TokenStore
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import uniffi.hummingbird_ffi_mobile.MobileGrillTurn
import uniffi.hummingbird_ffi_mobile.MobileGrillTurnState
import uniffi.hummingbird_ffi_mobile.grillRunBody
import uniffi.hummingbird_ffi_mobile.grillTurnIdle
import uniffi.hummingbird_ffi_mobile.grillTurnLine
import uniffi.hummingbird_ffi_mobile.grillTurnNoToken
import uniffi.hummingbird_ffi_mobile.grillTurnResponseFailed
import uniffi.hummingbird_ffi_mobile.grillTurnStarted
import uniffi.hummingbird_ffi_mobile.grillTurnStreamEnded
import uniffi.hummingbird_ffi_mobile.grillTurnTransportFailed

// One `grill-me` turn against the hosted runner (#538/M4), and the first
// HTTP client this module has ever had — every other byte on the wire goes
// through the Rust core's sync engine.
//
// **This file decides nothing.** What a line means, when a turn is over,
// what a decline says and what the request body's bytes are all live in
// `hummingbird_core::decisions::skills` and reach here as already-applied
// results (ADR-0025; `ffi-mobile/src/lib.rs`'s M4 section states the
// per-event carve-out). This file reports what happened to its socket — a
// line arrived, the request never resolved, the response was a 401, the
// stream ended — and emits whatever state comes back.
//
// **The lane cannot reach the sync engine**, and that is structural rather
// than intended: `SkillsLaneIsolationTest` fails the build if anything in
// this package names `CoreHolder`, the sync package, WorkManager or a
// dead-letter symbol. A sync mutation is a fact the reader already decided;
// a skill request is a question, and questions go stale (#269).
//
// **No cadence lives here.** No timer, no retry, no polling — the runner's
// 20s heartbeat is answered by de-duplication *in the core's reducer*, not
// by anything clock-shaped on this side. OkHttp's own connect/read
// deadlines are the one sanctioned exception, exactly as
// `AbortSignal.timeout` is on the web (ADR-0007's #274 amendment); see
// [defaultClient] for what each is set to and why.

/** The seven reducer entry points and the body builder, injectable.
 *
 * **This shape is not optional.** A plain JVM unit test cannot call a
 * uniffi function — there is no native library in that process — so a
 * runner that reached for the top-level bindings directly could not be
 * tested at all off-device, and the only automated evidence would be the
 * instrumented suite, which does not run in CI (there is no emulator).
 * `ItemDetailViewModel`'s `fetchFn`/`actFn`/`editFn` answer the same
 * problem the same way; `SkillsSeamTest` pins that the production default
 * really is the uniffi binding and not a fake.
 *
 * The *types* are safe to name in a JVM test — they are generated Kotlin
 * data classes, and constructing one loads nothing. */
data class GrillReducers(
    val idle: () -> MobileGrillTurnState = ::grillTurnIdle,
    val started: (MobileGrillTurnState) -> MobileGrillTurnState = ::grillTurnStarted,
    val line: (MobileGrillTurnState, String) -> MobileGrillTurnState = ::grillTurnLine,
    val noToken: (MobileGrillTurnState) -> MobileGrillTurnState = ::grillTurnNoToken,
    val transportFailed: (MobileGrillTurnState, String, Boolean) -> MobileGrillTurnState =
        ::grillTurnTransportFailed,
    val responseFailed: (MobileGrillTurnState, UShort) -> MobileGrillTurnState =
        ::grillTurnResponseFailed,
    val streamEnded: (MobileGrillTurnState) -> MobileGrillTurnState = ::grillTurnStreamEnded,
    val runBody: (String, List<MobileGrillTurn>) -> String = ::grillRunBody,
)

class SkillRunner(
    private val readToken: () -> String?,
    private val baseUrl: String = BuildConfig.AUTHORITY_BASE_URL,
    private val client: OkHttpClient = defaultClient(),
    private val reducers: GrillReducers = GrillReducers(),
) {

    /**
     * One turn of the interview, as the states it passes through. The flow
     * is cold: nothing is sent until it is collected, and cancelling the
     * collector cancels the `Call` — that is the `AbortController`
     * equivalent and the **only** way a run ends early.
     *
     * Total by construction: every exit emits a state. A caller needs no
     * try/catch, exactly as the web's `runSkill` promises for its own
     * generator.
     */
    fun grillTurn(ref: String, turns: List<MobileGrillTurn>): Flow<MobileGrillTurnState> =
        channelFlow {
            var state = reducers.started(reducers.idle())
            send(state)

            val token = readToken()
            if (token.isNullOrEmpty()) {
                // Pinned by a test: `execute()` is never called. There is
                // nothing to authenticate with, and issuing the request
                // anyway would spend a round trip to be told what is
                // already known.
                send(reducers.noToken(state))
                return@channelFlow
            }

            val request = Request.Builder()
                .url(baseUrl.trimEnd('/') + RUN_PATH)
                .header("Authorization", "Bearer $token")
                .post(reducers.runBody(ref, turns).toRequestBody(JSON))
                .build()

            val call = client.newCall(request)
            // The cancellation door, and the reason this is a `channelFlow`
            // rather than a plain `flow`.
            //
            // `Call.cancel()` is safe from any thread and is what actually
            // tears the socket down mid-stream. It cannot be hung off this
            // coroutine's own `invokeOnCompletion`, though: the loop below
            // is *blocked* in a socket read, so this job does not complete
            // until that read returns — which is the very thing being
            // waited on. A cancelled collector would leave the request
            // running against a runner that heartbeats for minutes.
            //
            // So the handler lives in a child coroutine that is *suspended*
            // rather than blocked. Cancelling the flow cancels it
            // immediately, its `finally` cancels the `Call`, the blocked
            // read throws, and the producer unwinds. This is the
            // `AbortController` equivalent and the only way a run ends
            // early — no timer, no retry.
            val cancellation = launch {
                try {
                    awaitCancellation()
                } finally {
                    call.cancel()
                }
            }

            // **Whether a backend answered, observed rather than inferred.**
            // `execute()` returning means a response arrived; a body that
            // tears afterwards does not take that back, and the run was
            // this backend's to lose rather than evidence it is
            // unreachable. Both cases throw the same `IOException`, so the
            // catch below cannot tell them apart and this flag is the only
            // thing that can. The web draws the identical line on `fetch`'s
            // resolve path (`route-run.ts`); nothing in this lane may
            // recover the difference from the decline's prose.
            var answered = false

            try {
                call.execute().use { response ->
                    answered = true
                    // A 400 or 413 from the runner is forwarded verbatim by
                    // the proxy and IS a valid terminal line, so status
                    // alone does not decide: the body is read either way,
                    // and only a body with nothing terminal in it falls
                    // back to the status.
                    val source = response.body?.source()
                    while (source != null) {
                        val line = source.readUtf8Line() ?: break
                        if (line.isEmpty()) continue
                        state = reducers.line(state, line)
                        send(state)
                    }
                    // Both of these are no-ops once the turn has settled —
                    // the core's reducer ignores an event in a terminal
                    // phase — so this needs no "did we see a terminal
                    // line?" flag of its own, and cannot disagree with the
                    // core about whether one arrived.
                    state =
                        if (response.isSuccessful) {
                            reducers.streamEnded(state)
                        } else {
                            reducers.responseFailed(state, response.code.toUShort())
                        }
                    send(state)
                }
            } catch (error: IOException) {
                // Offline, DNS, a connection reset, or a body that tore
                // mid-stream — the last of which is the case `answered`
                // exists for. A cancelled `Call` also lands here, and the
                // send below is simply never delivered, because the
                // collector is already gone.
                send(reducers.transportFailed(state, error.message.orEmpty(), answered))
            } finally {
                cancellation.cancel()
            }
        }.flowOn(Dispatchers.IO)

    companion object {
        /** The production wiring: the real device token, the real
         * authority origin, the real reducers ([GrillReducers]'s defaults).
         * `SkillsSeamTest` pins each of those three, because every one of
         * them is a place a fake could survive a green test run.
         *
         * [net.twinion.hummingbird.core.TokenStore] is the one thing this
         * lane borrows from `core/` — the credential at rest, not the sync
         * engine. `CoreHolder` and everything under `sync/` stay
         * unreachable from this package by construction. */
        fun create(context: Context): SkillRunner =
            SkillRunner(readToken = { TokenStore.load(context) })

        /** ADR-0018: same-origin path on the authority, which proxies to
         * the runner and forwards its NDJSON unchanged. */
        private const val RUN_PATH = "/api/skills/run"

        private val JSON = "application/json".toMediaType()

        /**
         * Three deadlines, each set against a number the lane actually has:
         *
         * - **connect, 5s** — a runner that cannot be reached should say so
         *   quickly rather than hanging the gesture. The web's own
         *   per-attempt connect deadline (`route-run.ts`) is 2500 ms; this
         *   is the same idea with a phone radio's wake-up in it.
         * - **read, 60s** — must exceed the runner's **20s** `"still
         *   running"` heartbeat, or every real run would die on its own
         *   progress. 60s is three beats of headroom.
         * - **call, 0 (none)** — a whole-run deadline would abort every
         *   real streamed run, which is exactly the argument
         *   `route-run.ts:171-183` makes for the web. The run ends when the
         *   runner answers or when the coroutine is cancelled; nothing else
         *   ends it.
         *
         * These are deadlines, not a cadence: each arms once and disarms
         * when the request settles. Nothing here repeats, reschedules or
         * polls, which is the line ADR-0007 draws.
         */
        fun defaultClient(): OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(5, TimeUnit.SECONDS)
            .readTimeout(60, TimeUnit.SECONDS)
            .callTimeout(0, TimeUnit.MILLISECONDS)
            .build()
    }
}
