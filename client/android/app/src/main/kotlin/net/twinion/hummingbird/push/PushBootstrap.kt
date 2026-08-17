package net.twinion.hummingbird.push

import android.content.Context
import com.google.firebase.FirebaseApp
import com.google.firebase.messaging.FirebaseMessaging

// The one place this app *asks* Firebase for anything (M2/#141).
//
// `HbMessagingService.onNewToken` covers the ordinary lifecycle — install,
// data clear, rotation — but not two real cases: an install that predates
// this app's Firebase configuration (there was no Firebase to issue a
// token, so the callback never fired), and a token that was issued but
// whose registration never reached the authority because no device token
// had been pasted yet. Both are resolved by asking for the current token
// at start and re-driving registration with it; the call is idempotent by
// slot id, so doing it every launch costs one cheap local read.
//
// **The `getApps` guard stays even though the json has landed.** Calling
// `FirebaseMessaging.getInstance()` with no initialised `FirebaseApp`
// throws, and `firebase-messaging` links fine without configuration — so a
// build that ever loses `google-services.json` (or a fork that strips it)
// degrades to "runs without push" instead of crashing at start. One cheap
// check against a crash-on-launch is worth keeping.
object PushBootstrap {

    fun refreshToken(context: Context) {
        val app = context.applicationContext
        if (FirebaseApp.getApps(app).isEmpty()) return
        FirebaseMessaging.getInstance().token.addOnSuccessListener { token ->
            if (token.isNullOrBlank()) return@addOnSuccessListener
            PushPrefs.saveFcmToken(app, token)
            RegistrationWorker.enqueue(app)
        }
    }
}
