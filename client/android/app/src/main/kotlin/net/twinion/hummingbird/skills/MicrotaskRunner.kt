package net.twinion.hummingbird.skills

import android.content.Context
import java.io.IOException
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
import uniffi.hummingbird_ffi_mobile.MobileSkillRunState
import uniffi.hummingbird_ffi_mobile.microtaskRunBody
import uniffi.hummingbird_ffi_mobile.skillRunIdle
import uniffi.hummingbird_ffi_mobile.skillRunLine
import uniffi.hummingbird_ffi_mobile.skillRunNoToken
import uniffi.hummingbird_ffi_mobile.skillRunResponseFailed
import uniffi.hummingbird_ffi_mobile.skillRunStarted
import uniffi.hummingbird_ffi_mobile.skillRunStreamEnded
import uniffi.hummingbird_ffi_mobile.skillRunTransportFailed

// One `microtask` run against the hosted runner (#273, landed on the phone
// at #539) — [SkillRunner]'s own twin, over the `skill_run_*` doors that
// #538 landed proven but unused. Same transport shape, same invariants:
// decides nothing (every report is an already-applied core state), cannot
// reach the sync engine (`SkillsLaneIsolationTest` covers this whole
// package, not just [SkillRunner]'s file), and no cadence of its own — see
// [SkillRunner]'s header for the fuller statement of each, which applies
// here unchanged.
//
// A second class rather than a generalised one: the two runs differ in
// their reducer set (`skill_run_*` has no `Question`/`Proposal` phase) and
// their body builder's shape (`microtaskRunBody`'s four scalar args against
// `grillRunBody`'s `ref`+`turns`), and folding both into one generic type
// would trade the "reducers are a plain constructor default" seam-test
// shape (`SkillsSeamTest`) for a generic parameter neither call site needs.

data class MicrotaskReducers(
    val idle: () -> MobileSkillRunState = ::skillRunIdle,
    val started: (MobileSkillRunState) -> MobileSkillRunState = ::skillRunStarted,
    val line: (MobileSkillRunState, String) -> MobileSkillRunState = ::skillRunLine,
    val noToken: (MobileSkillRunState) -> MobileSkillRunState = ::skillRunNoToken,
    val transportFailed: (MobileSkillRunState, String, Boolean) -> MobileSkillRunState =
        ::skillRunTransportFailed,
    val responseFailed: (MobileSkillRunState, UShort) -> MobileSkillRunState =
        ::skillRunResponseFailed,
    val streamEnded: (MobileSkillRunState) -> MobileSkillRunState = ::skillRunStreamEnded,
    val runBody: (String, Boolean, Long?, String?) -> String = { itemId, replace, grain, model ->
        microtaskRunBody(itemId, replace, grain, model)
    },
)

class MicrotaskRunner(
    private val readToken: () -> String?,
    private val baseUrl: String = BuildConfig.AUTHORITY_BASE_URL,
    private val client: OkHttpClient = SkillRunner.defaultClient(),
    private val reducers: MicrotaskReducers = MicrotaskReducers(),
) {

    /** One microtask run, as the states it passes through — [SkillRunner
     * .grillTurn]'s own shape: cold, total, and cancelling the collector is
     * the only way a run ends early. */
    fun run(
        itemId: String,
        replace: Boolean,
        grain: Long?,
        model: String?,
    ): Flow<MobileSkillRunState> = channelFlow {
        var state = reducers.started(reducers.idle())
        send(state)

        val token = readToken()
        if (token.isNullOrEmpty()) {
            send(reducers.noToken(state))
            return@channelFlow
        }

        val request = Request.Builder()
            .url(baseUrl.trimEnd('/') + RUN_PATH)
            .header("Authorization", "Bearer $token")
            .post(reducers.runBody(itemId, replace, grain, model).toRequestBody(JSON))
            .build()

        val call = client.newCall(request)
        val cancellation = launch {
            try {
                awaitCancellation()
            } finally {
                call.cancel()
            }
        }

        var answered = false

        try {
            call.execute().use { response ->
                answered = true
                val source = response.body?.source()
                while (source != null) {
                    val line = source.readUtf8Line() ?: break
                    if (line.isEmpty()) continue
                    state = reducers.line(state, line)
                    send(state)
                }
                state =
                    if (response.isSuccessful) {
                        reducers.streamEnded(state)
                    } else {
                        reducers.responseFailed(state, response.code.toUShort())
                    }
                send(state)
            }
        } catch (error: IOException) {
            send(reducers.transportFailed(state, error.message.orEmpty(), answered))
        } finally {
            cancellation.cancel()
        }
    }.flowOn(Dispatchers.IO)

    companion object {
        fun create(context: Context): MicrotaskRunner =
            MicrotaskRunner(readToken = { TokenStore.load(context) })

        private const val RUN_PATH = "/api/skills/run"

        private val JSON = "application/json".toMediaType()
    }
}
