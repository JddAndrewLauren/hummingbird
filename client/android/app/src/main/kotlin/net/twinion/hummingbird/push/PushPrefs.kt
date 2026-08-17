package net.twinion.hummingbird.push

import android.content.Context
import java.util.UUID

// This install's push identity: the slot id the authority keys a push
// target on, and the last FCM token seen for it (M2/#141).
//
// **Plain `SharedPreferences`, deliberately — not `EncryptedSharedPreferences`
// like `core.TokenStore`.** Neither value here is a credential:
//
// - The install id is an *idempotency key*. `MobileTaskHost.registerPushTarget`
//   documents it as a device **slot**: replaying it with a new `fcm_token`
//   adopts that token and clears `revoked_at`, while a fresh id per call would
//   accumulate dead slots that each receive a copy of every alert. Its whole
//   job is to be stable, and encrypting it works directly against that —
//   a Keystore master key can be invalidated (biometric enrolment changes,
//   a restore onto new hardware), and an unreadable store means a *new*
//   id, which is exactly the dead-slot failure the stable id exists to
//   prevent. Losing an idempotency key is worse than exposing one.
// - The FCM token is a routing address, not an authorisation: possessing
//   it lets nobody send to this device — sending needs `FCM_SERVICE_ACCOUNT`,
//   which never leaves the operator's terminal (CLAUDE.md's blast-radius
//   section). It is cached here only so registration can be re-driven when
//   the *device token* arrives later, without waiting for FCM to rotate.
//
// The device token itself stays in `core.TokenStore`, behind the Keystore,
// where it belongs.
object PushPrefs {
    private const val PREFS_FILE = "hummingbird-push"
    private const val KEY_INSTALL_ID = "install_id"
    private const val KEY_FCM_TOKEN = "fcm_token"

    private fun prefs(context: Context) =
        context.applicationContext.getSharedPreferences(PREFS_FILE, Context.MODE_PRIVATE)

    /** This install's stable push-target slot id, minted on first ask and
     * never re-minted. Every registration for this install must send this
     * same value (see the note above on dead slots). */
    fun installId(context: Context): String {
        val store = prefs(context)
        store.getString(KEY_INSTALL_ID, null)?.let { return it }
        // A tiny race is possible (two threads both minting); `commit`
        // ordering does not fix it, so re-read under a lock and keep
        // whichever landed first — the invariant that matters is that the
        // value never changes once anything has *sent* it.
        synchronized(this) {
            store.getString(KEY_INSTALL_ID, null)?.let { return it }
            val minted = UUID.randomUUID().toString()
            store.edit().putString(KEY_INSTALL_ID, minted).apply()
            return minted
        }
    }

    fun saveFcmToken(context: Context, token: String) {
        prefs(context).edit().putString(KEY_FCM_TOKEN, token).apply()
    }

    fun loadFcmToken(context: Context): String? =
        prefs(context).getString(KEY_FCM_TOKEN, null)
}
