package net.twinion.hummingbird.push

import androidx.work.WorkManager
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import net.twinion.hummingbird.notify.AlertNotification
import net.twinion.hummingbird.notify.AlertNotifier
import net.twinion.hummingbird.sync.SyncWorker

// The FCM entry point (M2/#141). Two jobs: keep the registered token
// current, and turn an arriving alert into a notification.
//
// `onNewToken` fires on install, on a data-clear, and whenever FCM rotates
// — never on a schedule this app controls. Registration is deliberately not
// done inline here: the service runs on a short-lived callback with no
// network guarantee, and a failed registration would be lost. It caches the
// token and hands the actual `POST` to [RegistrationWorker], which is
// network-constrained and retries.
class HbMessagingService : FirebaseMessagingService() {

    override fun onNewToken(token: String) {
        super.onNewToken(token)
        PushPrefs.saveFcmToken(applicationContext, token)
        RegistrationWorker.enqueue(applicationContext)
    }

    // Every arriving alert, since the payload is data-only and FCM renders
    // nothing itself (#141's payload commit; `fcm.rs`'s own test pins the
    // absence of a `notification` block). Post, enqueue, return — this
    // callback touches no core and no network, because it runs on a
    // short-lived service the OS may stop the moment it returns.
    override fun onMessageReceived(message: RemoteMessage) {
        super.onMessageReceived(message)
        AlertNotification.from(message.data)?.let {
            AlertNotifier.post(applicationContext, it)
        }
        // Unconditional, even when the payload mapped to nothing showable:
        // the *arrival* of a push is the evidence that this device's
        // mirror is behind, and the alerts surface reads the mirror. It is
        // also what makes the Ack action work — the ack is CAS against a
        // row this cycle is what brings down.
        WorkManager.getInstance(applicationContext).enqueue(SyncWorker.expeditedOnPush())
    }
}
