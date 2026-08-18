package net.twinion.hummingbird.skills

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.SocketPolicy
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileSkillRunState

// [MicrotaskRunner]'s own share of #538's automated evidence — the same
// "fake reducers, real transport" split [SkillRunnerTest] documents, over
// the `skill_run_*` doors instead of the `grill_turn_*` ones. Not a full
// re-run of every case there: the transport mechanism (cold flow,
// cancel-on-collector-gone, `answered` split on the `IOException`) is the
// SAME code path proven exhaustively by that suite; what differs here —
// the reducer set and the four-argument body builder — is what this file
// actually exercises.
@OptIn(ExperimentalCoroutinesApi::class)
class MicrotaskRunnerTest {

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

    private class FakeReducers {
        val reports = mutableListOf<String>()

        fun asReducers(bodyText: String = BODY): MicrotaskReducers = MicrotaskReducers(
            idle = { MobileSkillRunState.Idle },
            started = { record("started") },
            line = { _, line -> record("line:$line") },
            noToken = { record("noToken") },
            transportFailed = { _, detail, answered ->
                record("transportFailed:${detail.isNotEmpty()}:answered=$answered")
            },
            responseFailed = { _, status -> record("responseFailed:$status") },
            streamEnded = { record("streamEnded") },
            runBody = { _, _, _, _ -> bodyText },
        )

        private fun record(what: String): MobileSkillRunState {
            reports += what
            return MobileSkillRunState.Running(reports.toList())
        }
    }

    private fun runner(
        fake: FakeReducers,
        token: String? = "a-device-token",
        body: String = BODY,
    ) = MicrotaskRunner(
        readToken = { token },
        baseUrl = server.url("/").toString().trimEnd('/'),
        client = SkillRunner.defaultClient(),
        reducers = fake.asReducers(body),
    )

    @Test
    fun `a streamed run reports every line then the end of the stream`() = runTest {
        server.enqueue(
            MockResponse().setBody(
                """
                {"type":"progress","message":"reading"}
                {"ok":true,"skill":"microtask","result":{"steps":["a"],"note":"n"},"backend":"b","model":"m"}
                """.trimIndent() + "\n",
            ),
        )
        val fake = FakeReducers()

        runner(fake).run("i", false, null, null).toList()

        assertEquals(
            listOf(
                "started",
                """line:{"type":"progress","message":"reading"}""",
                """line:{"ok":true,"skill":"microtask","result":{"steps":["a"],"note":"n"},"backend":"b","model":"m"}""",
                "streamEnded",
            ),
            fake.reports,
        )
    }

    @Test
    fun `the request carries the cores body verbatim and the bearer token`() = runTest {
        server.enqueue(MockResponse().setBody("""{"ok":true,"result":{}}""" + "\n"))
        val fake = FakeReducers()

        runner(fake, body = """{"skill":"microtask","args":{"ref":"i"}}""")
            .run("i", false, null, null)
            .toList()

        val request = server.takeRequest()
        assertEquals("POST", request.method)
        assertEquals("/api/skills/run", request.path)
        assertEquals("Bearer a-device-token", request.getHeader("Authorization"))
        assertEquals("""{"skill":"microtask","args":{"ref":"i"}}""", request.body.readUtf8())
    }

    @Test
    fun `a run request threads the callers own arguments into the body builder`() = runTest {
        server.enqueue(MockResponse().setBody("""{"ok":true,"result":{}}""" + "\n"))
        var seen: List<Any?>? = null
        val fake = FakeReducers()
        val reducers = fake.asReducers().copy(runBody = { itemId, replace, grain, model ->
            seen = listOf(itemId, replace, grain, model)
            BODY
        })

        MicrotaskRunner(
            readToken = { "t" },
            baseUrl = server.url("/").toString().trimEnd('/'),
            client = SkillRunner.defaultClient(),
            reducers = reducers,
        ).run("i", true, 3L, "m").toList()

        assertEquals(listOf("i", true, 3L, "m"), seen)
    }

    @Test
    fun `no token means no request is ever sent`() = runTest {
        val fake = FakeReducers()

        runner(fake, token = null).run("i", false, null, null).toList()

        assertEquals(listOf("started", "noToken"), fake.reports)
        assertEquals(0, server.requestCount)
    }

    @Test
    fun `an unauthorized response reports its status`() = runTest {
        server.enqueue(MockResponse().setResponseCode(401))
        val fake = FakeReducers()

        runner(fake).run("i", false, null, null).toList()

        assertEquals(listOf("started", "responseFailed:401"), fake.reports)
    }

    @Test
    fun `a refused connection reports a transport failure nothing answered`() = runTest {
        server.enqueue(MockResponse().setSocketPolicy(SocketPolicy.DISCONNECT_AT_START))
        val fake = FakeReducers()

        runner(fake).run("i", false, null, null).toList()

        assertEquals(listOf("started", "transportFailed:true:answered=false"), fake.reports)
    }

    private companion object {
        const val BODY = """{"skill":"microtask","args":{"ref":"i"}}"""
    }
}
