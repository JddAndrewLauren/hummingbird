package net.twinion.hummingbird.wear.token

import androidx.work.WorkManager
import com.google.android.gms.wearable.MessageEvent
import com.google.android.gms.wearable.WearableListenerService
import kotlinx.coroutines.runBlocking
import net.twinion.hummingbird.core.CoreHolder
import net.twinion.hummingbird.core.TokenMessage
import net.twinion.hummingbird.core.TokenStore
import net.twinion.hummingbird.sync.SyncWorker

// The watch's half of the token hand-off (ADR-0039). The phone's Settings
// sends the `device-watch` token once, over the Data Layer, on
// `TokenMessage.PATH`; the system starts this service for it (the manifest's
// `MESSAGE_RECEIVED` filter, scoped to the `/hummingbird` prefix), and the
// token is stored, pushed to the core, announced to the open home screen,
// and used at once. **Pushed, not rehydrated**: `MobileTaskHost.pushApiKey`
// is the entry that resumes a credential hold (`Core::push_api_key`'s own
// doc), and a watch that has been sitting in `held` since install is the
// normal case here — `rehydrateApiKey` is for the token the process already
// had at start (`CoreHolder`).
//
// Nothing else arrives on this channel and nothing else is accepted: a
// message on another path is ignored, and bytes that do not parse as a
// token (`TokenMessage.parse`) are dropped without touching the store.
class TokenListenerService : WearableListenerService() {

    override fun onMessageReceived(event: MessageEvent) {
        if (event.path != TokenMessage.PATH) return
        val token = TokenMessage.parse(event.data) ?: return
        val context = applicationContext
        TokenStore.save(context, token)
        // `WearableListenerService` callbacks run on a binder thread, not
        // the main one; blocking it for the core's handshake is what the
        // service is for, and the alternative — a scope that outlives the
        // callback — would let the process be reclaimed mid-push.
        runBlocking { CoreHolder.get(context).pushApiKey(token) }
        TokenPresence.refresh(context)
        // The first sync with a credential, now rather than at the next
        // hourly tick — `"user"` reaches the core as `Trigger::User`, which
        // is not backoff-gated (`SyncWorker`'s own doc on the trigger).
        WorkManager.getInstance(context).enqueue(SyncWorker.oneShot(SyncWorker.TRIGGER_USER))
    }
}
