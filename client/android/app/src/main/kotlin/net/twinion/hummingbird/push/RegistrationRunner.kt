package net.twinion.hummingbird.push

import android.content.Context
import android.os.Build
import net.twinion.hummingbird.core.CoreHolder
import uniffi.hummingbird_ffi_mobile.MobilePushRegistrationException

/** What one registration attempt decided — [RegistrationWorker]'s whole
 * output, kept free of `androidx.work` so the decision can be tested on a
 * plain JVM. */
enum class RegistrationOutcome {
    /** Registered, or nothing to do. Do not run again. */
    DONE,

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
// - **`Unauthorized` is `DONE`, not `RETRY`.** It means this device has no
//   accepted device token yet — a state only a human pasting one into the
//   Status screen can leave. Retrying on a backoff would burn wakeups
//   against a condition no amount of waiting changes; the *arrival* of a
//   token is the event that re-drives registration, which is why
//   `MainActivity`'s `onSaveToken` enqueues this work itself.
// - **`RegisterFailed` is `RETRY`, safely.** The authority keys a push
//   target on the client-supplied slot id and re-registering with the same
//   id is idempotent (`MobileTaskHost.registerPushTarget`'s doc), so a
//   retry after an ambiguous failure cannot create a second target or
//   duplicate a ring.
class RegistrationRunner(
    private val installIdFn: () -> String,
    private val fcmTokenFn: () -> String?,
    private val registerFn: suspend (id: String, name: String, token: String) -> Unit,
) {

    suspend fun run(): RegistrationOutcome {
        // No FCM token cached yet: `onNewToken` (or the first token fetch)
        // will enqueue this work again when there is one. Nothing to
        // register, and nothing a retry would find.
        val token = fcmTokenFn() ?: return RegistrationOutcome.DONE
        return try {
            registerFn(installIdFn(), Build.MODEL ?: "android", token)
            RegistrationOutcome.DONE
        } catch (_: MobilePushRegistrationException.Unauthorized) {
            RegistrationOutcome.DONE
        } catch (_: MobilePushRegistrationException.RegisterFailed) {
            RegistrationOutcome.RETRY
        }
    }

    companion object {
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
