package net.twinion.hummingbird

import android.app.Application
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import java.util.concurrent.TimeUnit
import net.twinion.hummingbird.diagnostics.DiagnosticsRecorder
import net.twinion.hummingbird.notify.NotificationChannels
import net.twinion.hummingbird.push.PushBootstrap
import net.twinion.hummingbird.sync.SyncWorker

class HummingbirdApp : Application() {

    override fun onCreate() {
        super.onCreate()
        // #709: this is what makes the process's one monotonic origin get
        // sampled at actual process start rather than by whichever of the
        // four writers happens to record first — `get` samples it eagerly
        // on this thread (see `DiagnosticsRecorder.Companion.create`, which
        // states why nothing there may be lazy). Mints no event itself —
        // `session.started` stays `CoreHolder`'s.
        DiagnosticsRecorder.get(this)
        scheduleHourlySync()
        // Before any push can arrive: a notification posted against a
        // channel id that does not exist is dropped to a default channel
        // with no warning on a release build (see NotificationChannels).
        NotificationChannels.ensure(this)
        // A no-op if Firebase is somehow unconfigured (see PushBootstrap's
        // own note on the getApps guard).
        PushBootstrap.refreshToken(this)
    }

    // The ~hourly OS-deferred refresh (#141 sync model). KEEP: rescheduling
    // an already-scheduled job on every app open would reset its cadence —
    // WorkManager owns this clock, nothing else does (the no-competing-
    // clocks rule; the foreground 60s cadence in `MainActivity`'s `AppRoot`
    // is the *other* leg — live whenever the app is foregrounded,
    // regardless of which screen is showing, exactly as on web).
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
