package net.twinion.hummingbird.core

import android.content.Context
import java.io.File
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import net.twinion.hummingbird.BuildConfig
import uniffi.hummingbird_ffi_mobile.MobileTaskHost

// One durable core per process (ADR-0010's one-core discipline, in its
// Android form): the activity and the WorkManager sync job share this
// handle rather than each loading the queue and mirror themselves. The
// namespace is app-local files — the host's one init-time contribution
// (ADR-0003) — and the stored device token is rehydrated (not pushed:
// rehydration never resumes a credential hold; see Core::rehydrate_api_key).
object CoreHolder {
    @Volatile
    private var host: MobileTaskHost? = null
    private val initLock = Mutex()

    suspend fun get(context: Context): MobileTaskHost {
        host?.let { return it }
        return initLock.withLock {
            host ?: create(context.applicationContext).also { host = it }
        }
    }

    private suspend fun create(context: Context): MobileTaskHost {
        val namespace = File(context.filesDir, "hummingbird-core").absolutePath
        // Init with no key, then rehydrate the stored one: MobileTaskHost.init
        // treats its api_key as a push (first entry), and app start is a
        // reload of a token we already had.
        val core = MobileTaskHost.init(namespace, BuildConfig.AUTHORITY_BASE_URL, "")
        TokenStore.load(context)?.let { core.rehydrateApiKey(it) }
        return core
    }
}
