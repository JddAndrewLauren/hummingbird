package net.twinion.hummingbird.wear

import android.app.Application
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import java.util.concurrent.TimeUnit
import net.twinion.hummingbird.core.SyncHistoryStore
import net.twinion.hummingbird.diagnostics.DiagnosticsRecorder
import net.twinion.hummingbird.sync.SyncWorker
import net.twinion.hummingbird.wear.tile.TileRefresh
import uniffi.hummingbird_ffi_mobile.isInformativeSyncOutcome

// The watch's Application (ADR-0039): the phone's `HummingbirdApp` minus
// everything the watch does not have — no notification channels, no push
// registration, no Firebase. What is left is the two things every device
// does at process start: bring up the one diagnostics recorder, and own the
// hourly background leg of the sync model.
class WearApp : Application() {
    override fun onCreate() {
        super.onCreate()
        DiagnosticsRecorder.get(this)
        // The background leg's aftermath (the 2026-09-10 design handoff):
        // the cycle goes into the sync history the tile's count line is
        // judged against — the worker itself writes no history, the phone's
        // foreground root does that there — and the tile is asked to
        // redraw. Set once, at process start, before any worker can run.
        SyncWorker.onRunFinished = { context, outcomeKind, nowMs ->
            if (isInformativeSyncOutcome(outcomeKind)) {
                SyncHistoryStore.recordInformative(context, outcomeKind, nowMs)
            }
            TileRefresh.request(context)
        }
        scheduleHourlySync()
    }

    // The ~hourly OS-deferred refresh (#141 sync model), verbatim from the
    // phone. KEEP: rescheduling an already-scheduled job on every app open
    // would reset its cadence — WorkManager owns this clock, nothing else
    // does (the no-competing-clocks rule; the foreground 60s cadence in
    // `MainActivity`'s `WearAppRoot` is the *other* leg). Between the two,
    // a watch that is never opened is at most an hour stale, and the home
    // screen says so rather than hiding it.
    private fun scheduleHourlySync() {
        val request = PeriodicWorkRequestBuilder<SyncWorker>(1, TimeUnit.HOURS)
            .setConstraints(
                Constraints.Builder()
                    .setRequiredNetworkType(NetworkType.CONNECTED)
                    .build(),
            )
            .build()
        WorkManager.getInstance(this).enqueueUniquePeriodicWork(
            "hourly-sync",
            ExistingPeriodicWorkPolicy.KEEP,
            request,
        )
    }
}
