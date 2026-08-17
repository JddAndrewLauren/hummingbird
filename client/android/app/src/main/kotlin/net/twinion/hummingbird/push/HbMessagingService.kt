package net.twinion.hummingbird.push

import com.google.firebase.messaging.FirebaseMessagingService

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
}
