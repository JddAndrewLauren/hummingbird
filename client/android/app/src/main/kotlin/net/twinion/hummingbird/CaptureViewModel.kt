package net.twinion.hummingbird

import android.content.Context
import androidx.lifecycle.ViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.twinion.hummingbird.core.CoreHolder
import uniffi.hummingbird_ffi_mobile.canSubmitCapture

// M1-5's whole surface (#128/#503): the "ViewModel over CoreHolder" pattern
// the brief names, holding the one field's draft and deciding whether it is
// worth submitting. `canSubmitFn` defaults, in [create], to the uniffi
// `canSubmitCapture` door onto `hummingbird_core::decisions::can_submit_capture`
// (ADR-0025) — never a hand-rolled blank-string check, which disagrees with
// the real rule on a BOM-only draft (see `hummingbird_core::decisions::capture`'s
// doc for the exact case). It is injected, not called directly, purely so a
// JVM test can exercise [submit]'s control flow (refuses without capturing,
// captures on a real draft, propagates the given clock) without touching
// `hummingbird_ffi_mobile`'s generated JNI binding — a plain JVM process has
// no host-architecture `.so` to load (`CoreBindingTest`'s own doc: that
// round trip is a device/emulator-only check). `ManifestAliasTest`'s sibling
// `CaptureSubmitRefusalTest` is the mechanical proof that [create] really
// does wire the real uniffi fn and that neither this file nor
// `CaptureActivity.kt` re-derives the rule locally.
//
// **Repository layer deferred** (#503's own scope note): `captureFn` is the
// other seam this ViewModel needs, and [create] closes it over `CoreHolder`
// directly — the same shape `MainActivity`'s `ProofScreen` already uses. A
// repository abstraction is future work once a second screen needs one, not
// invented ahead of that need.
//
// `submit` is `suspend`, not self-launched on `viewModelScope`: the caller
// (`CaptureActivity`, or a JVM test) controls the coroutine and can await
// completion before e.g. finishing the activity, and a JVM test needs no
// `Dispatchers.Main` wiring to call it directly.
class CaptureViewModel(
    private val canSubmitFn: (String) -> Boolean,
    private val captureFn: suspend (title: String, nowMs: Long) -> String,
) : ViewModel() {

    private val _draft = MutableStateFlow("")
    val draft: StateFlow<String> = _draft.asStateFlow()

    /** Whether `text` (the current draft by default) is worth submitting. */
    fun canSubmit(text: String = _draft.value): Boolean = canSubmitFn(text)

    fun onDraftChange(value: String) {
        _draft.value = value
    }

    /** The mic button's raw transcript lands here verbatim (ADR-0022: no
     * parsing on the way in), replacing whatever was typed so far — the
     * same "the box holds one draft" model the web capture box uses. */
    fun onTranscript(transcript: String) {
        _draft.value = transcript
    }

    /** Captures the current draft if [canSubmit] says it is worth it;
     * returns whether it did. Local-first per #128's own criterion:
     * [captureFn] (`MobileTaskHost.capture`, in production) enqueues
     * durably before any network call, so a caller awaiting this can finish
     * the activity immediately after — the item is already in the local
     * mirror. */
    suspend fun submit(nowMs: Long): Boolean {
        val title = _draft.value
        if (!canSubmit(title)) {
            return false
        }
        captureFn(title, nowMs)
        return true
    }

    companion object {
        /** The production wiring: [canSubmitFn] is the uniffi
         * `canSubmitCapture` function reference verbatim, and [captureFn]
         * closes over the app's one durable [CoreHolder] handle — never a
         * fresh core per capture. */
        fun create(context: Context): CaptureViewModel =
            CaptureViewModel(
                canSubmitFn = ::canSubmitCapture,
                captureFn = { title, nowMs ->
                    CoreHolder.get(context.applicationContext).capture(title, nowMs)
                },
            )
    }
}
