package net.twinion.nanorunner

import android.app.Application
import android.os.Build
import android.provider.Settings
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import java.io.File
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/**
 * What the screen is allowed to know.
 *
 * BLINDNESS IS A HARD REQUIREMENT: the operator may still owe the bake-off a blind
 * ground-truth labelling when this runs, so no parse content, no error text and no raw
 * model output may ever reach the UI. Counts and ids only.
 */
sealed interface UiState {
    val harnessCommit: String get() = BuildConfig.HARNESS_COMMIT

    data object Checking : UiState
    data class Unavailable(val detail: String) : UiState
    data object DownloadNeeded : UiState
    data class Downloading(val downloaded: Long, val total: Long?) : UiState
    data class Ready(val total: Int, val alreadyDone: Int, val dryRun: Boolean) : UiState
    data class Running(val done: Int, val total: Int, val errors: Int, val dryRun: Boolean) : UiState
    data class Done(
        val total: Int,
        val ok: Int,
        val errors: Int,
        val failedIds: List<String>,
        val dryRun: Boolean,
        val stoppedEarly: String? = null,
    ) : UiState

    /** A problem with the app itself (assets, corrupt results file) — safe to show. */
    data class Broken(val detail: String) : UiState
}

class RunnerViewModel(app: Application) : AndroidViewModel(app) {

    private val _state = MutableStateFlow<UiState>(UiState.Checking)
    val state: StateFlow<UiState> = _state.asStateFlow()

    private var engine: NanoEngine? = null
    private var runJob: Job? = null
    var dryRun: Boolean = false
        private set

    private lateinit var captures: List<Capture>
    private lateinit var template: String
    private lateinit var schemaText: String

    private val outputDir: File get() = getApplication<Application>().getExternalFilesDir(null)!!

    /** The three files the operator ships back. Dry runs get their own names — always. */
    val resultsFile: File get() = File(outputDir, prefix() + "nano_results.jsonl")
    val rawFile: File get() = File(outputDir, prefix() + "nano_raw.jsonl")
    val metaFile: File get() = File(outputDir, prefix() + "nano_run_meta.json")

    private fun prefix() = if (dryRun) "dryrun_" else ""

    fun start(dryRun: Boolean) {
        this.dryRun = dryRun
        if (engine != null) return // configuration change (the Fold does this a lot)
        engine = if (dryRun) FakeEngine() else MlKitEngine()
        viewModelScope.launch { check() }
    }

    private suspend fun check() {
        _state.value = UiState.Checking
        try {
            val assets = getApplication<Application>().assets
            template = assets.open("harness/prompt.md").use { it.readBytes().toString(Charsets.UTF_8) }
            schemaText = assets.open("harness/schema.json").use { it.readBytes().toString(Charsets.UTF_8) }
            captures = Corpus.parse(
                assets.open("harness/corpus.jsonl").use { it.readBytes().toString(Charsets.UTF_8) }
            )
        } catch (e: CancellationException) {
            throw e
        } catch (t: Throwable) {
            _state.value = UiState.Broken("bundled harness assets unreadable: ${Rows.describe(t)}")
            return
        }

        val done = try {
            ResultsFile(resultsFile).use { it.repairAndScanIds() }
        } catch (e: CancellationException) {
            throw e
        } catch (t: Throwable) {
            _state.value = UiState.Broken(Rows.describe(t))
            return
        }

        when (availabilityOrBroken() ?: return) {
            Availability.AVAILABLE -> _state.value = UiState.Ready(captures.size, done.size, dryRun)
            Availability.DOWNLOADABLE -> _state.value = UiState.DownloadNeeded
            Availability.DOWNLOADING -> _state.value = UiState.DownloadNeeded
            Availability.UNAVAILABLE -> _state.value = UiState.Unavailable(
                "Gemini Nano is not available on this device (no AICore, or the device " +
                    "or system version doesn't support it)."
            )
        }
    }

    private suspend fun availabilityOrBroken(): Availability? = try {
        engine!!.availability()
    } catch (e: CancellationException) {
        throw e
    } catch (t: Throwable) {
        _state.value = UiState.Unavailable(Rows.describe(t))
        null
    }

    /** True once there is anything to discard — drives the "start over" affordance. */
    fun hasResults(): Boolean = resultsFile.isFile && resultsFile.length() > 0

    /**
     * Throw away this run and start clean. Needed because a recorded id is never re-run:
     * when a run is spoiled by something that isn't the model (a bad generation config, a
     * quota storm), the only honest recovery is to discard it rather than resume into it.
     */
    fun discardResults() {
        if (runJob?.isActive == true) return
        listOf(resultsFile, rawFile, metaFile).forEach { runCatching { it.delete() } }
        viewModelScope.launch { check() }
    }

    /** Must happen on Wi-Fi, BEFORE airplane mode — this is the one networked step. */
    fun download() {
        if (runJob?.isActive == true) return
        runJob = viewModelScope.launch {
            _state.value = UiState.Downloading(0, null)
            try {
                engine!!.download().collect { p ->
                    _state.value = UiState.Downloading(p.downloaded, p.total)
                }
                check()
            } catch (e: CancellationException) {
                throw e
            } catch (t: Throwable) {
                _state.value = UiState.Unavailable("model download failed: ${Rows.describe(t)}")
            }
        }
    }

    /**
     * The run loop. Corpus order, each id exactly once, one fsync'd row per capture.
     * A throw from the engine is a recorded result, not an abort: the bake-off wants to
     * know how often on-device inference fails, so the loop only ever moves forward.
     */
    fun run() {
        if (runJob?.isActive == true) return
        runJob = viewModelScope.launch {
            val startedAt = System.currentTimeMillis()
            val airplaneAtStart = airplaneModeOn()
            _state.value = UiState.Running(0, captures.size, 0, dryRun)

            val summary = try {
                ResultsFile(resultsFile).use { results ->
                    ResultsFile(rawFile).use { raw ->
                        RunLoop(engine!!, captures, template, schemaText, results, raw).run { p ->
                            _state.value = UiState.Running(p.done, p.total, p.errors, dryRun)
                        }
                    }
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: EngineMisconfigured) {
                _state.value = UiState.Broken(
                    "${e.message}: ${Rows.describe(e.cause ?: e)}\n\nNothing was recorded — " +
                        "this is the app's fault, not the model's, and it would have " +
                        "failed identically on all ${captures.size} captures."
                )
                return@launch
            } catch (t: Throwable) {
                _state.value = UiState.Broken(Rows.describe(t))
                return@launch
            }

            // A resume that had nothing left to do must not overwrite the metadata of
            // the run that actually produced the results.
            if (summary.skipped < summary.total) {
                writeMeta(startedAt, airplaneAtStart, summary)
            }
            _state.value = UiState.Done(
                summary.total, summary.ok, summary.errors, summary.failedIds, dryRun,
                summary.stoppedEarly,
            )
        }
    }

    /**
     * Honest offline evidence: the bake-off's claim is that Nano parsed these captures
     * with the radios off, so the flag is sampled from the system at run start rather
     * than asserted in prose afterwards.
     */
    private fun airplaneModeOn(): Boolean = runCatching {
        Settings.Global.getInt(
            getApplication<Application>().contentResolver,
            Settings.Global.AIRPLANE_MODE_ON,
            0,
        ) != 0
    }.getOrDefault(false)

    private suspend fun writeMeta(
        startedAt: Long,
        airplaneAtStart: Boolean,
        summary: RunLoop.Summary,
    ) {
        val engineFacts = runCatching { engine!!.describe() }.getOrDefault(emptyMap())
        val meta = buildJsonObject {
            put("harness_commit", BuildConfig.HARNESS_COMMIT)
            put("app_version", BuildConfig.VERSION_NAME)
            put("dry_run", dryRun)
            put("device", "${Build.MANUFACTURER} ${Build.MODEL}")
            put("android_release", Build.VERSION.RELEASE)
            put("android_sdk_int", Build.VERSION.SDK_INT)
            put("build_fingerprint", Build.FINGERPRINT)
            put("airplane_mode_at_start", airplaneAtStart)
            put("started_at_epoch_ms", startedAt)
            put("finished_at_epoch_ms", System.currentTimeMillis())
            put("captures", captures.size)
            put("ok", summary.ok)
            put("failed", summary.errors)
            put("skipped_already_present", summary.skipped)
            put(
                "failed_ids",
                kotlinx.serialization.json.JsonArray(summary.failedIds.map { JsonPrimitive(it) }),
            )
            put("engine", JsonObject(engineFacts.mapValues { JsonPrimitive(it.value) }))
        }
        runCatching { metaFile.writeText(meta.toString() + "\n") }
    }

    override fun onCleared() {
        engine?.close()
        engine = null
    }
}
