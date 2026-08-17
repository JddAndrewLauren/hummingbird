package net.twinion.hummingbird.push

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.work.WorkManager
import net.twinion.hummingbird.notify.AlertNotifier

// The notification's Ack action lands here (M2/#141, ADR-0012).
//
// A broadcast receiver rather than an Activity: acking from the shade must
// not open the app, and Android 12+ bans the reverse anyway (a notification
// trampoline). It does the minimum and returns — **no `goAsync`**. The ack
// is sync-then-CAS with a retry (see [AckRunner]), which can exceed a
// receiver's ~10-second budget, and a receiver killed mid-flight loses the
// gesture silently. WorkManager is the thing that survives that.
class AckReceiver : BroadcastReceiver() {

    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != ACTION_ACK) return
        val alertId = intent.getStringExtra(AlertNotifier.EXTRA_ALERT_ID) ?: return
        WorkManager.getInstance(context.applicationContext)
            .enqueue(AckWorker.expedited(alertId))
    }

    companion object {
        const val ACTION_ACK = "net.twinion.hummingbird.action.ACK_ALERT"
    }
}
