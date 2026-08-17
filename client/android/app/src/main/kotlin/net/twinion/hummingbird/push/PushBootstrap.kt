package net.twinion.hummingbird.push

import android.content.Context
import com.google.firebase.FirebaseApp
import com.google.firebase.messaging.FirebaseMessaging

// The one place this app *asks* Firebase for anything (M2/#141).
//
// `HbMessagingService.onNewToken` covers the ordinary lifecycle — install,
// data clear, rotation — but not two real cases: the app was installed
// before `google-services.json` existed (there was no Firebase to issue a
// token, so the callback never fired), and a token that was issued but
// whose registration never reached the authority because no device token
// had been pasted yet. Both are resolved by asking for the current token
// at start and re-driving registration with it; the call is idempotent by
// slot id, so doing it every launch costs one cheap local read.
//
// **The `getApps` guard is what lets this land before the json does.** The
// `firebase-messaging` dependency compiles and links with no
// `google-services.json` — only the `com.google.gms.google-services`
// plugin needs the file, and this build does not apply it — but calling
// `FirebaseMessaging.getInstance()` with no initialised `FirebaseApp`
// throws. So: if Firebase was never configured, there is nothing to ask
// and the app simply runs without push, rather than crashing at start.
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
