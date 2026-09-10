package net.twinion.hummingbird.core

import android.content.Context
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey

// The device token at rest. A `device` token is the authority's only
// read-capable scope and is write-everything — a write credential however
// read-only it looks (CLAUDE.md's blast-radius rule) — so it rests behind
// the Android Keystore (EncryptedSharedPreferences over a hardware-backed
// master key), not in plain prefs or a file.
object TokenStore {
    private const val PREFS_FILE = "hummingbird-credentials"
    private const val KEY_DEVICE_TOKEN = "device_token"

    private fun prefs(context: Context) =
        EncryptedSharedPreferences.create(
            context,
            PREFS_FILE,
            MasterKey.Builder(context).setKeyScheme(MasterKey.KeyScheme.AES256_GCM).build(),
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
        )

    /** The stored token, or null if none has ever been saved. */
    fun load(context: Context): String? =
        prefs(context).getString(KEY_DEVICE_TOKEN, null)

    fun save(context: Context, token: String) {
        prefs(context).edit().putString(KEY_DEVICE_TOKEN, token).apply()
    }

    fun clear(context: Context) {
        prefs(context).edit().remove(KEY_DEVICE_TOKEN).apply()
    }
}
