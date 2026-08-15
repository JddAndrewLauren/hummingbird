package net.twinion.hummingbird

import android.app.Application
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import java.util.concurrent.TimeUnit
import net.twinion.hummingbird.sync.SyncWorker

class HummingbirdApp : Application() {

    override fun onCreate() {
        super.onCreate()
        scheduleHourlySync()
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
