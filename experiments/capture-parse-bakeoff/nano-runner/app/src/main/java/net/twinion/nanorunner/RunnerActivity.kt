package net.twinion.nanorunner

import android.app.AlertDialog
import android.content.Intent
import android.os.Bundle
import android.view.View
import android.view.WindowManager
import android.widget.Button
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.viewModels
import androidx.core.content.FileProvider
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import java.io.File
import kotlinx.coroutines.launch

/**
 * One screen, one button. The Fold recreates this activity on fold/unfold, which is why
 * the run lives in the ViewModel's scope — a lifecycleScope coroutine would be cancelled
 * mid-inference and the operator would watch the counter reset.
 */
class RunnerActivity : ComponentActivity() {

    private val vm: RunnerViewModel by viewModels()

    private lateinit var stateView: TextView
    private lateinit var detailView: TextView
    private lateinit var failedView: TextView
    private lateinit var primary: Button
    private lateinit var share: Button
    private lateinit var startOver: Button
    private lateinit var footer: TextView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_runner)
        stateView = findViewById(R.id.state)
        detailView = findViewById(R.id.detail)
        failedView = findViewById(R.id.failed_ids)
        primary = findViewById(R.id.primary)
        share = findViewById(R.id.share)
        startOver = findViewById(R.id.start_over)
        footer = findViewById(R.id.footer)

        // Debug-only dry run: `adb shell am start -n net.twinion.nanorunner/.RunnerActivity
        //                     --ez dryRun true`
        val dryRun = BuildConfig.DEBUG && intent.getBooleanExtra("dryRun", false)
        vm.start(dryRun)

        share.setOnClickListener { shareResults() }

        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                vm.state.collect { render(it) }
            }
        }
    }

    private fun render(state: UiState) {
        keepScreenOn(state is UiState.Running || state is UiState.Downloading)
        failedView.text = ""
        share.visibility = View.GONE
        startOver.visibility = View.GONE
        primary.isEnabled = false
        footer.text = "harness ${BuildConfig.HARNESS_COMMIT} · ${vm.resultsFile.parent}"

        when (state) {
            is UiState.Checking -> {
                stateView.text = "Checking model availability…"
                detailView.text = ""
            }

            is UiState.Unavailable -> {
                stateView.text = "Gemini Nano unavailable"
                detailView.text = state.detail +
                    "\n\nThis is a device/AICore fact, not a bake-off result — nothing was run."
            }

            is UiState.DownloadNeeded -> {
                stateView.text = "Model download needed"
                detailView.text = "Do this on Wi-Fi. It MUST finish before you turn on " +
                    "airplane mode — it is the only step that needs the network."
                primary.text = getString(R.string.download)
                primary.isEnabled = true
                primary.setOnClickListener { vm.download() }
            }

            is UiState.Downloading -> {
                stateView.text = "Downloading model…"
                val total = state.total
                detailView.text = if (total != null && total > 0) {
                    "%,d / %,d bytes — must finish before airplane mode".format(
                        state.downloaded, total,
                    )
                } else {
                    "%,d bytes — must finish before airplane mode".format(state.downloaded)
                }
            }

            is UiState.Ready -> {
                stateView.text = if (state.dryRun) "Ready (DRY RUN)" else "Ready"
                if (state.alreadyDone > 0) showStartOver()
                detailView.text = buildString {
                    append("${state.total} captures")
                    if (state.alreadyDone > 0) {
                        append(" · ${state.alreadyDone} already recorded, will be skipped")
                    }
                    if (state.dryRun) append("\n\nDRY RUN — fake outputs, dryrun_* files.")
                    append("\n\nTurn on airplane mode and Wi-Fi off before starting.")
                }
                primary.text = getString(R.string.start)
                primary.isEnabled = true
                primary.setOnClickListener { vm.run() }
            }

            is UiState.Running -> {
                stateView.text = if (state.dryRun) "Running (DRY RUN)" else "Running"
                detailView.text = "${state.done} / ${state.total} · errors: ${state.errors}" +
                    "\n\nKeep the screen on. Safe to leave and come back — finished " +
                    "captures are never re-run."
            }

            is UiState.Done -> {
                stateView.text = when {
                    state.stoppedEarly != null -> "Stopped early"
                    state.dryRun -> "Done (DRY RUN)"
                    else -> "Done"
                }
                detailView.text = buildString {
                    append("${state.total} captures · ${state.ok} recorded parses · ")
                    append("${state.errors} failures")
                    state.stoppedEarly?.let { append("\n\n").append(it) }
                }
                showStartOver()
                if (state.failedIds.isNotEmpty()) {
                    failedView.text = "failed ids: " + state.failedIds.joinToString(", ")
                }
                share.visibility = View.VISIBLE
                primary.text = getString(R.string.start)
                primary.isEnabled = true
                primary.setOnClickListener { vm.run() } // no-op unless rows are missing
            }

            is UiState.Broken -> {
                stateView.text = "Can't run"
                detailView.text = state.detail
                if (vm.hasResults()) showStartOver()
            }
        }
    }

    /**
     * Discarding is destructive and irreversible, so it is always a two-tap decision.
     * The confirmation names the files rather than showing any of their content.
     */
    private fun showStartOver() {
        startOver.visibility = View.VISIBLE
        startOver.setOnClickListener {
            AlertDialog.Builder(this)
                .setTitle("Discard this run?")
                .setMessage(
                    "Deletes nano_results.jsonl, nano_raw.jsonl and nano_run_meta.json " +
                        "from this phone and starts the 42 captures over.\n\nOnly do this " +
                        "if the run was spoiled by something other than the model — " +
                        "genuine model failures are results and should be kept."
                )
                .setNegativeButton("Keep") { d, _ -> d.dismiss() }
                .setPositiveButton("Discard") { _, _ -> vm.discardResults() }
                .show()
        }
    }

    private fun keepScreenOn(on: Boolean) {
        if (on) {
            window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        } else {
            window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        }
    }

    /** All three files at once — results, the verbatim audit sidecar, and the metadata. */
    private fun shareResults() {
        val files = listOf(vm.resultsFile, vm.rawFile, vm.metaFile).filter(File::isFile)
        if (files.isEmpty()) return
        val uris = ArrayList(
            files.map { FileProvider.getUriForFile(this, "$packageName.fileprovider", it) }
        )
        val intent = Intent(Intent.ACTION_SEND_MULTIPLE).apply {
            type = "application/json"
            putParcelableArrayListExtra(Intent.EXTRA_STREAM, uris)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        startActivity(Intent.createChooser(intent, "Send bake-off results"))
    }
}
