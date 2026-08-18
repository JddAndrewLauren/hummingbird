package net.twinion.hummingbird.skills

import java.io.IOException
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import okhttp3.Call
import okhttp3.EventListener
import okhttp3.OkHttpClient
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.SocketPolicy
import okio.Buffer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileGrillQuestion
import uniffi.hummingbird_ffi_mobile.MobileGrillTurn
import uniffi.hummingbird_ffi_mobile.MobileGrillTurnState

// The automated half of #538's evidence. A plain JVM process has no native
// library loaded, so **no uniffi function can be called here** — the
// reducers are fakes and what is under test is the transport: what it
// sends, which report it makes for which socket outcome, and that
// cancelling the collector cancels the `Call`.
//
// The *rules* those reports feed into are tested in Rust
// (`hummingbird_core::decisions::skills`, `ffi-mobile`'s `skills_tests`),
// and the whole lane end to end only on hardware (`GrillTurnProbeTest`,
// operator-run — there is no emulator in CI, so a green badge here is
// deliberately not a claim about a real run).
@OptIn(ExperimentalCoroutinesApi::class)
class SkillRunnerTest {

    private lateinit var server: MockWebServer

    @Before
    fun start() {
        server = MockWebServer()
        server.start()
    }

    @After
    fun stop() {
        server.shutdown()
    }

    /** A fake state machine, standing in for the core's reducer: it records
     * the events the transport reported, in order. The states themselves
     * are the real generated types, which cost nothing to construct. */
    private class FakeReducers {
        val reports = mutableListOf<String>()

        fun state(vararg messages: String): MobileGrillTurnState =
            MobileGrillTurnState.Asking(messages.toList())

        fun asReducers(bodyText: String = BODY): GrillReducers = GrillReducers(
            idle = { MobileGrillTurnState.Idle },
            started = { record("started") },
            line = { _, line -> record("line:$line") },
            noToken = { record("noToken") },
            transportFailed = { _, detail -> record("transportFailed:${detail.isNotEmpty()}") },
            responseFailed = { _, status -> record("responseFailed:$status") },
            streamEnded = { record("streamEnded") },
            runBody = { _, _ -> bodyText },
        )

        private fun record(what: String): MobileGrillTurnState {
            reports += what
            return MobileGrillTurnState.Asking(reports.toList())
        }
    }

    private fun runner(
        fake: FakeReducers,
        token: String? = "a-device-token",
        client: OkHttpClient = SkillRunner.defaultClient(),
        body: String = BODY,
    ) = SkillRunner(
        readToken = { token },
        baseUrl = server.url("/").toString().trimEnd('/'),
        client = client,
        reducers = fake.asReducers(body),
    )

    @Test
    fun `a streamed turn reports every line then the end of the stream`() = runTest {
        server.enqueue(
            MockResponse()
                .setBody(
                    """
                    {"type":"progress","message":"still running"}
                    {"type":"progress","message":"still running"}
                    {"ok":true,"skill":"grill-me","result":{"kind":"question"},"backend":"b","model":"m"}
                    """.trimIndent() + "\n",
                )
                .setHeader("content-type", "application/x-ndjson"),
        )
        val fake = FakeReducers()

        runner(fake).grillTurn("HB-42", emptyList()).toList()

        assertEquals(
            listOf(
                "started",
                """line:{"type":"progress","message":"still running"}""",
                """line:{"type":"progress","message":"still running"}""",
                """line:{"ok":true,"skill":"grill-me","result":{"kind":"question"},"backend":"b","model":"m"}""",
                "streamEnded",
            ),
            fake.reports,
        )
    }

    /** The heartbeat is reported line by line and collapsed by the CORE, not
     * here — the transport must not de-duplicate on its own, or the two
     * clients would disagree about a narration. The three identical lines
     * above each reached `line`, which is that proof. */
    @Test
    fun `the request carries the core's body verbatim, and the bearer token`() = runTest {
        server.enqueue(MockResponse().setBody("""{"ok":true,"result":{}}""" + "\n"))
        val fake = FakeReducers()

        runner(fake, body = """{"skill":"grill-me","args":{"ref":"i","turns":[]}}""")
            .grillTurn("i", emptyList())
            .toList()

        val request = server.takeRequest()
        assertEquals("POST", request.method)
        assertEquals("/api/skills/run", request.path)
        assertEquals("Bearer a-device-token", request.getHeader("Authorization"))
        // Byte for byte what the reducer handed over — the transport builds
        // no body of its own. The shared fixture
        // (`client/core/tests/fixtures/skills-run-bodies.json`) is what pins
        // those bytes; this pins that they arrive unmodified.
        assertEquals(
            """{"skill":"grill-me","args":{"ref":"i","turns":[]}}""",
            request.body.readUtf8(),
        )
    }

    @Test
    fun `a turn's own turns list reaches the body builder untrimmed`() = runTest {
        server.enqueue(MockResponse().setBody("""{"ok":true,"result":{}}""" + "\n"))
        var seen: List<MobileGrillTurn>? = null
        val fake = FakeReducers()
        val reducers = fake.asReducers().copy(runBody = { _, turns ->
            seen = turns
            BODY
        })
        val turns = listOf(
            MobileGrillTurn(
                question = MobileGrillQuestion("Which airport?", "SEA", listOf("SEA", "PDX")),
                answer = "SEA",
            ),
        )

        SkillRunner(
            readToken = { "t" },
            baseUrl = server.url("/").toString().trimEnd('/'),
            client = SkillRunner.defaultClient(),
            reducers = reducers,
        ).grillTurn("i", turns).toList()

        assertEquals(turns, seen)
    }

    @Test
    fun `no token means no request is ever sent`() = runTest {
        val fake = FakeReducers()

        runner(fake, token = null).grillTurn("i", emptyList()).toList()

        assertEquals(listOf("started", "noToken"), fake.reports)
        assertEquals("nothing may be sent without a credential", 0, server.requestCount)
    }

    @Test
    fun `an empty token is treated as no token`() = runTest {
        val fake = FakeReducers()

        runner(fake, token = "").grillTurn("i", emptyList()).toList()

        assertEquals(listOf("started", "noToken"), fake.reports)
        assertEquals(0, server.requestCount)
    }

    /** An empty-bodied 401 has no terminal line to read, so the status is
     * the only thing that can be said — and the core is what says it. */
    @Test
    fun `an unauthorized response reports its status`() = runTest {
        server.enqueue(MockResponse().setResponseCode(401))
        val fake = FakeReducers()

        runner(fake).grillTurn("i", emptyList()).toList()

        assertEquals(listOf("started", "responseFailed:401"), fake.reports)
    }

    /** A non-2xx whose body IS a terminal line still reaches `line` — the
     * proxy forwards a runner 400 verbatim (ADR-0018), and status alone
     * must not decide. The trailing `responseFailed` is a no-op against the
     * real reducer, which has already settled. */
    @Test
    fun `a non-2xx body is still read as lines`() = runTest {
        server.enqueue(
            MockResponse().setResponseCode(400).setBody("""{"ok":false,"error":"bad ref"}""" + "\n"),
        )
        val fake = FakeReducers()

        runner(fake).grillTurn("i", emptyList()).toList()

        assertEquals(
            listOf("started", """line:{"ok":false,"error":"bad ref"}""", "responseFailed:400"),
            fake.reports,
        )
    }

    @Test
    fun `a refused connection reports a transport failure`() = runTest {
        server.enqueue(MockResponse().setSocketPolicy(SocketPolicy.DISCONNECT_AT_START))
        val fake = FakeReducers()

        runner(fake).grillTurn("i", emptyList()).toList()

        assertEquals(listOf("started", "transportFailed:true"), fake.reports)
    }

    /** A last line with no trailing newline is still a line — okio's
     * `readUtf8Line` answers it on the way to end-of-stream, which is what
     * makes this the platform twin of the web's final `TextDecoder` flush. */
    @Test
    fun `a terminal line with no trailing newline is still read`() = runTest {
        server.enqueue(MockResponse().setBody("""{"ok":true,"result":{"kind":"question"}}"""))
        val fake = FakeReducers()

        runner(fake).grillTurn("i", emptyList()).toList()

        assertEquals(
            listOf("started", """line:{"ok":true,"result":{"kind":"question"}}""", "streamEnded"),
            fake.reports,
        )
    }

    @Test
    fun `blank lines between records are skipped rather than classified`() = runTest {
        server.enqueue(MockResponse().setBody("\n\n" + """{"ok":true,"result":{}}""" + "\n\n"))
        val fake = FakeReducers()

        runner(fake).grillTurn("i", emptyList()).toList()

        assertEquals(listOf("started", """line:{"ok":true,"result":{}}""", "streamEnded"), fake.reports)
    }

    /**
     * Cancellation mid-stream — the third case the hardware probe repeats
     * for real. The response body is throttled so the stream is still open
     * when the collector stops after the first progress line; the `Call`
     * must be cancelled and nothing may be emitted afterwards.
     */
    @Test
    fun `cancelling the collector cancels the call and emits nothing further`() = runTest {
        val body = Buffer()
            .writeUtf8("""{"type":"progress","message":"one"}""" + "\n")
            .writeUtf8("""{"type":"progress","message":"two"}""" + "\n")
            .writeUtf8("""{"ok":true,"result":{}}""" + "\n")
        server.enqueue(
            MockResponse()
                .setBody(body)
                // Slow enough that the take(2) below lands mid-stream.
                .throttleBody(16, 1, TimeUnit.SECONDS),
        )
        val fake = FakeReducers()
        // OkHttp itself is the witness: a cancelled `Call` fails, and
        // `callFailed` is the event that says so. A latch rather than a
        // sleep — the cancellation crosses from the collector's thread to
        // the IO thread, and polling for it would be the flaky version of
        // this test.
        val failed = CountDownLatch(1)
        val client = SkillRunner.defaultClient().newBuilder()
            .eventListener(object : EventListener() {
                override fun callFailed(call: Call, ioe: IOException) {
                    if (call.isCanceled()) failed.countDown()
                }
            })
            .build()

        // `take(2)` cancels the flow as soon as the second state arrives:
        // the "started" state and the first line.
        val seen = runner(fake, client = client).grillTurn("i", emptyList()).take(2).toList()

        assertEquals("nothing may be emitted after the collector goes away", 2, seen.size)
        assertNotNull(seen.first())
        assertTrue(
            "the Call must be cancelled when the collector goes away",
            failed.await(10, TimeUnit.SECONDS),
        )
        // The run ended where it was cancelled, not at the runner's own
        // terminal line: the response was still streaming, so no end-of-
        // stream report was ever made.
        assertFalse("the stream must not have been allowed to finish", fake.reports.contains("streamEnded"))
        assertTrue(
            "the run must stop near the cancellation, not read the whole body",
            fake.reports.size < 4,
        )
        // **`flowOn` buffers one item.** The producer can be a single state
        // ahead of the collector, so a report for the line after the last
        // emitted one is legitimate and is not a leak: it was computed and
        // dropped, never delivered. What matters — and what is asserted
        // above — is that nothing reached the collector and the socket is
        // gone. Asserting an exact report list here would be asserting a
        // buffer size.
    }

    private companion object {
        const val BODY = """{"skill":"grill-me","args":{"ref":"i","turns":[]}}"""
    }
}
