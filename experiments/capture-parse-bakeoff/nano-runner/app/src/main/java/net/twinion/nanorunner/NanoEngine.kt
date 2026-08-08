package net.twinion.nanorunner

import com.google.mlkit.genai.common.DownloadStatus
import com.google.mlkit.genai.common.FeatureStatus
import com.google.mlkit.genai.prompt.GenerateContentRequest
import com.google.mlkit.genai.prompt.Generation
import com.google.mlkit.genai.prompt.GenerativeModel
import com.google.mlkit.genai.prompt.TextPart
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map

/** Model availability, in the app's own vocabulary. */
enum class Availability { UNAVAILABLE, DOWNLOADABLE, DOWNLOADING, AVAILABLE }

/** Bytes so far / total, or null totals when the SDK hasn't said yet. */
data class DownloadProgress(val downloaded: Long, val total: Long?, val done: Boolean)

/**
 * The seam between the run loop and Gemini Nano, so the loop (resume, fsync, blindness)
 * is testable on the JVM and the emulator can exercise the plumbing without AICore.
 */
interface NanoEngine {
    suspend fun availability(): Availability
    fun download(): Flow<DownloadProgress>

    /** @return the model's output text VERBATIM. Throws on API failure. */
    suspend fun generate(prompt: String): String

    /** Free-form facts about the engine, recorded in nano_run_meta.json. */
    suspend fun describe(): Map<String, String>

    fun close()
}

/**
 * The real thing: ML Kit GenAI Prompt API, `com.google.mlkit:genai-prompt`.
 *
 * Deliberately UNCONSTRAINED generation with the shared prompt verbatim. The SDK's
 * structured-output path (`GenerateTypedContentRequest`, `@Generable`) is alpha, is
 * compile-time Kotlin classes only, cannot express this schema (no regex, no
 * cross-field `items[0] == title`, no "absent-or-minItems-2"), gives no guaranteed
 * constrained decoding, and injects its own schema wording into the prompt — which
 * would break the identical-prompt invariant that makes the bake-off a comparison at
 * all. So: plain `generateContent`, output recorded verbatim, validated post-hoc by the
 * same Python validator the hosted side uses. That asymmetry is itself a finding.
 *
 * Sampling params are left at SDK defaults on purpose — setting temperature/topK/seed
 * would be tuning the Nano side in a way the hosted side never got.
 */
class MlKitEngine : NanoEngine {

    private val model: GenerativeModel by lazy { Generation.getClient() }

    override suspend fun availability(): Availability = when (model.checkStatus()) {
        FeatureStatus.AVAILABLE -> Availability.AVAILABLE
        FeatureStatus.DOWNLOADABLE -> Availability.DOWNLOADABLE
        FeatureStatus.DOWNLOADING -> Availability.DOWNLOADING
        else -> Availability.UNAVAILABLE
    }

    override fun download(): Flow<DownloadProgress> {
        var total: Long? = null
        return model.download().map { status ->
            when (status) {
                is DownloadStatus.DownloadStarted -> {
                    total = status.bytesToDownload
                    DownloadProgress(0, total, done = false)
                }
                is DownloadStatus.DownloadProgress ->
                    DownloadProgress(status.totalBytesDownloaded, total, done = false)
                is DownloadStatus.DownloadCompleted ->
                    DownloadProgress(total ?: 0, total, done = true)
                is DownloadStatus.DownloadFailed -> throw status.e
            }
        }
    }

    override suspend fun generate(prompt: String): String {
        val request = GenerateContentRequest.builder(TextPart(prompt))
            .apply {
                // Headroom, not tuning: >5x the largest hosted parse. If a response is
                // still cut off, that truncation is recorded as an honest failure.
                maxOutputTokens = MAX_OUTPUT_TOKENS
                candidateCount = 1
            }
            .build()
        val response = model.generateContent(request)
        val candidate = response.candidates.firstOrNull()
            ?: throw IllegalStateException("model returned no candidates")
        return candidate.text
    }

    override suspend fun describe(): Map<String, String> = buildMap {
        put("engine", "mlkit-genai-prompt")
        put("sdk_version", BuildConfig.MLKIT_GENAI_PROMPT_VERSION)
        put("max_output_tokens", MAX_OUTPUT_TOKENS.toString())
        put("candidate_count", "1")
        put("sampling", "sdk defaults (temperature/topK/seed unset)")
        put("structured_output", "not used — see NanoEngine.kt")
        runCatching { model.getBaseModelName() }.onSuccess { put("base_model", it) }
        runCatching { model.getTokenLimit() }.onSuccess { put("token_limit", it.toString()) }
        runCatching { availability() }.onSuccess { put("feature_status", it.name) }
    }

    override fun close() {
        runCatching { model.close() }
    }

    companion object {
        const val MAX_OUTPUT_TOKENS = 1024
    }
}

/**
 * Dry-run engine for the emulator and for exercising the run loop's plumbing. Debug
 * builds only, opt-in via `--ez dryRun true`. It writes to `dryrun_*.jsonl`, never the
 * real filenames, so fake data can never be mistaken for a bake-off result.
 */
class FakeEngine : NanoEngine {

    private var n = 0

    override suspend fun availability() = Availability.AVAILABLE

    override fun download(): Flow<DownloadProgress> = flow {
        emit(DownloadProgress(0, 100, done = false))
        emit(DownloadProgress(100, 100, done = true))
    }

    override suspend fun generate(prompt: String): String {
        n++
        return when (n % 7) {
            // A lenient-JSON output: org.json would have accepted this, the strict
            // parser must not — that difference is exactly what this row proves.
            3 -> "{'title': 'single quotes are not JSON'}"
            // An API failure mid-run.
            5 -> throw IllegalStateException("fake engine failure #$n")
            // A fenced answer: valid JSON inside, still a failure by the rules.
            6 -> "```json\n{\"title\": \"fenced output\"}\n```"
            else -> "{\"title\": \"fake parse $n\", \"notes\": \"dry run — not a result\"}"
        }
    }

    override suspend fun describe(): Map<String, String> = mapOf(
        "engine" to "fake (dry run)",
        "warning" to "DRY RUN — these are not model outputs",
    )

    override fun close() = Unit
}
