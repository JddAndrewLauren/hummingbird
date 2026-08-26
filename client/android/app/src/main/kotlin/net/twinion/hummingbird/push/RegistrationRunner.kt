package net.twinion.hummingbird.push

import android.content.Context
import android.os.Build
import android.util.Log
import net.twinion.hummingbird.core.CoreHolder
import uniffi.hummingbird_ffi_mobile.MobilePushRegistrationException

/** What one registration attempt decided — [RegistrationWorker]'s whole
 * output, kept free of `androidx.work` so the decision can be tested on a
 * plain JVM.
 *
 * #519: three of the four cases below are DONE-shaped —
 * [RegistrationWorker] maps every one of them to `Result.success()`, and
 * that contract does not change here. What changes is that
 * [RegistrationRunner.run] no longer collapses them into one shared value
 * before returning: a registered
 * device, a device with nothing to register yet, and a device the authority
 * has not accepted a token for are three different situations that read
 * identically as `DONE` from the caller's side. Each now has its own
 * outcome and its own logcat line, so which one happened is a read, not a
 * process of elimination through the debugger. */
enum class RegistrationOutcome {
    /** Registered. Do not run again. */
    REGISTERED,

    /** No FCM token cached yet, so nothing to register. Do not run again
     * — `onNewToken` (or the first token fetch) will enqueue this work
     * again when there is one. */
    NO_TOKEN,

    /** The authority has no accepted device token for this install yet.
     * Do not run again on a backoff; only a human pasting one into the
     * Status screen changes this. */
    UNAUTHORIZED,

    /** A transport-level failure; try again on WorkManager's backoff. */
    RETRY,
}

// The registration decision, lifted out of [RegistrationWorker] the same
// way `NowViewModel` lifts its reads out of `NowScreen` — injected fns, so
// a JVM test drives the control flow with no `.so`, no WorkManager and no
// Firebase (`CaptureViewModel`'s own reasoning, applied to a worker).
//
// The two outcomes are not symmetric, and the asymmetry is the point:
//
// - **`Unauthorized` is `UNAUTHORIZED`, not `RETRY`.** It means this device
//   has no accepted device token yet — a state only a human pasting one
//   into the Status screen can leave. Retrying on a backoff would burn
//   wakeups against a condition no amount of waiting changes; the
//   *arrival* of a token is the event that re-drives registration, which
//   is why `MainActivity`'s `onSaveToken` enqueues this work itself.
// - **`RegisterFailed` is `RETRY`, safely.** The authority keys a push
//   target on the client-supplied slot id and re-registering with the same
//   id is idempotent (`MobileTaskHost.registerPushTarget`'s doc), so a
//   retry after an ambiguous failure cannot create a second target or
//   duplicate a ring.
class RegistrationRunner(
    private val installIdFn: () -> String,
    private val fcmTokenFn: () -> String?,
    private val registerFn: suspend (id: String, name: String, token: String) -> Unit,
    /** Injected the same way as the other collaborators above, so a JVM
     * test can capture what would have gone to logcat without a mocked
     * `android.util.Log` or Robolectric. This class set the per-class-`TAG`
     * convention that `client/android/README.md`'s "Production logging
     * (main/)" section now documents for the rest of `main/`. */
    private val logFn: (String) -> Unit = { msg -> Log.i(TAG, msg) },
) {

    suspend fun run(): RegistrationOutcome {
        // No FCM token cached yet: `onNewToken` (or the first token fetch)
        // will enqueue this work again when there is one. Nothing to
        // register, and nothing a retry would find.
        val token = fcmTokenFn() ?: run {
            logFn("no cached FCM token; nothing to register")
            return RegistrationOutcome.NO_TOKEN
        }
        return try {
            registerFn(installIdFn(), Build.MODEL ?: "android", token)
            logFn("registered")
            RegistrationOutcome.REGISTERED
        } catch (_: MobilePushRegistrationException.Unauthorized) {
            logFn("unauthorized -- no accepted device token yet")
            RegistrationOutcome.UNAUTHORIZED
        } catch (_: MobilePushRegistrationException.RegisterFailed) {
            logFn("registration failed; retrying")
            RegistrationOutcome.RETRY
        }
    }

    companion object {
        private const val TAG = "RegistrationRunner"

        /** The production wiring: the persisted slot id and cached FCM
         * token, against the app's one durable core handle. */
        fun create(context: Context): RegistrationRunner {
            val app = context.applicationContext
            return RegistrationRunner(
                installIdFn = { PushPrefs.installId(app) },
                fcmTokenFn = { PushPrefs.loadFcmToken(app) },
                registerFn = { id, name, token ->
                    CoreHolder.get(app).registerPushTarget(id, name, token)
                },
            )
        }
    }
}
