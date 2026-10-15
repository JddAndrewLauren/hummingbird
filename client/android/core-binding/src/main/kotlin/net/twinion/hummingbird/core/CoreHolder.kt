package net.twinion.hummingbird.core

import android.content.Context
import java.io.File
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import net.twinion.hummingbird.diagnostics.DiagnosticsRecorder
import uniffi.hummingbird_ffi_mobile.MobileDiagnosticEvent
import uniffi.hummingbird_ffi_mobile.MobileTaskHost

// One durable core per process (ADR-0010's one-core discipline, in its
// Android form): the activity and the WorkManager sync job share this
// handle rather than each loading the queue and mirror themselves. The
// namespace is app-local files — the host's one init-time contribution
// (ADR-0003) — and the stored device token is rehydrated (not pushed:
// rehydration never resumes a credential hold; see Core::rehydrate_api_key).
// With no token stored, or a store that cannot be read, the absence is
// reported instead, so the core's credential always leaves `unknown` (#846).
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
        // Init takes no key (#846); app start is a reload of a token we
        // already had, or a report that there is none.
        val core = MobileTaskHost.init(namespace, BuildConfig.AUTHORITY_BASE_URL)
        val stored = runCatching { TokenStore.load(context) }.getOrNull()
        if (stored != null) core.rehydrateApiKey(stored) else core.reportNoApiKey()
        // The mobile FFI host's own diagnostic touch point (#709): this is
        // the moment the app's one durable core handle for this process
        // comes into existence, which is what `session.started` means here
        // — Android has no other session lifecycle to hang it on.
        DiagnosticsRecorder.get(context).record(MobileDiagnosticEvent.SessionStarted)
        return core
    }
}
