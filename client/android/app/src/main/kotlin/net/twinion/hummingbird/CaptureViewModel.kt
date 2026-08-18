package net.twinion.hummingbird

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.twinion.hummingbird.core.CoreHolder
import uniffi.hummingbird_ffi_mobile.CaptureDestination
import uniffi.hummingbird_ffi_mobile.CaptureDraft
import uniffi.hummingbird_ffi_mobile.CaptureFormMeta
import uniffi.hummingbird_ffi_mobile.MetaProblems
import uniffi.hummingbird_ffi_mobile.canSubmitCapture
import uniffi.hummingbird_ffi_mobile.captureFormMeta
import uniffi.hummingbird_ffi_mobile.captureMetaProblems

/** The capture box's whole draft (#529), one Kotlin value shadowing
 * [CaptureDraft] field-for-field — every optional field a plain `String`,
 * `""` meaning "not set", the same convention [CaptureDraft]'s own doc
 * states for the seam. Kept as its own type rather than holding a
 * [CaptureDraft] directly so this ViewModel's state has a `data class`
 * default constructor and `copy()`, neither of which a uniffi-generated
 * record reliably carries across binding versions.
 */
data class CaptureFormState(
    val title: String = "",
    val destination: CaptureDestination = CaptureDestination.TRIAGE,
    val size: String = "",
    val energy: String = "",
    val context: String = "",
    val description: String = "",
    val projectId: String = "",
    val priority: String = "",
    val deadline: String = "",
    val scheduledDate: String = "",
) {
    fun toDraft(): CaptureDraft = CaptureDraft(
        title = title,
        destination = destination,
        size = size,
        energy = energy,
        context = context,
        description = description,
        projectId = projectId,
        priority = priority,
        deadline = deadline,
        scheduledDate = scheduledDate,
    )
}

// M1-5's whole surface (#128/#503), widened at M3/#529 to the capture box's
// full field set: destination, energy/size, context, and the details
// disclosure (description, project, priority, deadline, scheduled date).
// `canSubmitFn`/`metaProblemsFn` default, in [create], to the uniffi doors
// onto `hummingbird_core::decisions::capture` (ADR-0025) — never a
// hand-rolled blank-string check or a hand-rolled date regex, either of
// which is an automatic reject in this repo's own gate
// (`CaptureSubmitRefusalTest`). `formMetaFn` is the third door #529 adds:
// the vocabulary and context suggestions the fields render, read once per
// screen (a per-gesture cost, never a per-row one — `ffi-mobile::lib.rs`'s
// own module doc). All three are injected, not called directly, purely so
// a JVM test can exercise this ViewModel's control flow without touching
// `hummingbird_ffi_mobile`'s generated JNI binding — see
// `CaptureViewModel.kt`'s original doc for why (a plain JVM process has no
// host-architecture `.so` to load). `CaptureSubmitRefusalTest`'s sibling is
// the mechanical proof that [create] really does wire the real uniffi fns.
//
// **Repository layer deferred** (#503's own scope note, unchanged at #529):
// `captureFn` is the other seam this ViewModel needs, and [create] closes
// it over `CoreHolder` directly — the same shape `MainActivity`'s
// `ProofScreen` already uses.
//
// `submit` is `suspend`, not self-launched on `viewModelScope`: the caller
// (`CaptureActivity`, or a JVM test) controls the coroutine and can await
// completion before e.g. finishing the activity, and a JVM test needs no
// `Dispatchers.Main` wiring to call it directly.
class CaptureViewModel(
    private val canSubmitFn: (String) -> Boolean,
    private val metaProblemsFn: (deadline: String, scheduledDate: String) -> MetaProblems,
    private val formMetaFn: () -> CaptureFormMeta,
    private val captureFn: suspend (draft: CaptureDraft, nowMs: Long) -> String,
) : ViewModel() {

    private val _draft = MutableStateFlow(CaptureFormState())
    val draft: StateFlow<CaptureFormState> = _draft.asStateFlow()

    /** The vocabulary and context suggestions the fields render — read once,
     * lazily, on first use rather than in the constructor, so a JVM test
     * that never touches the form's metadata never calls [formMetaFn]. */
    val formMeta: CaptureFormMeta by lazy { formMetaFn() }

    /** The last dictation attempt's failure, or `null` if the mic is idle
     * or the last attempt produced a transcript. ADR-0022 requires every
     * way dictation can end without text — unavailable, refused permission,
     * a mid-session error, no match — to end the session *and tell the
     * reader*; "silently unavailable is a defect against this ADR, not a
     * nit" (that ADR's own words). Held here rather than in a composable's
     * `remember` so it survives the same Activity recreation the draft does,
     * and so a JVM test can assert the transitions without a device. */
    private val _dictationFailure = MutableStateFlow<DictationFailure?>(null)
    val dictationFailure: StateFlow<DictationFailure?> = _dictationFailure.asStateFlow()

    /** What is wrong with the draft's two free-text dates right now, by
     * field — the core's answer, shown next to the field it belongs to.
     * Dictation stays title-only (#529's own boundary): nothing here reads
     * a spoken transcript into anything but [CaptureFormState.title]. */
    val metaProblems: MetaProblems
        get() = metaProblemsFn(_draft.value.deadline, _draft.value.scheduledDate)

    /** Whether `title` (the current draft's by default) is worth
     * submitting at all — the title rule alone, matching the pre-#529
     * surface's `canSubmit` name and shape so `CaptureActivity`'s existing
     * call sites (the Enter-key handler, the button's `enabled`) need no
     * change beyond the wider `submit`. [canSubmitDraft] is the fuller
     * check `submit` actually gates on. */
    fun canSubmit(title: String = _draft.value.title): Boolean = canSubmitFn(title)

    /** Whether the whole draft — title plus both free-text dates — can be
     * submitted right now. A malformed date is refused here rather than
     * sent for the authority to 400 into the dead-letter journal, the same
     * discipline [ItemDetailViewModel.canSave] already applies to an edit. */
    fun canSubmitDraft(): Boolean {
        val problems = metaProblems
        return canSubmit() && problems.deadline == null && problems.scheduledDate == null
    }

    fun updateDraft(draft: CaptureFormState) {
        _draft.value = draft
    }

    /** Clears any previous notice as a fresh attempt begins — a stale "no
     * match" hanging over a listening mic reads as the new attempt already
     * having failed. */
    fun onDictationStarted() {
        _dictationFailure.value = null
    }

    fun onDictationFailed(failure: DictationFailure) {
        _dictationFailure.value = failure
    }

    /** The mic button's raw transcript lands here verbatim (ADR-0022: no
     * parsing on the way in), replacing whatever was typed in the title so
     * far — dictation stays title-field-only; every other field is
     * unaffected (#529's own boundary, carried from M1-5's title-only
     * surface). */
    fun onTranscript(transcript: String) {
        _draft.value = _draft.value.copy(title = transcript)
        _dictationFailure.value = null
    }

    /** Captures the current draft if [canSubmitDraft] says it is worth it;
     * returns whether it did. Local-first per #128's own criterion:
     * [captureFn] (`MobileTaskHost.capture`, in production) enqueues
     * durably before any network call, so a caller awaiting this can finish
     * the activity immediately after — the item is already in the local
     * mirror. */
    suspend fun submit(nowMs: Long): Boolean {
        val current = _draft.value
        if (!canSubmitDraft()) {
            return false
        }
        captureFn(current.toDraft(), nowMs)
        return true
    }

    companion object {
        /** The production wiring: [canSubmitFn]/[metaProblemsFn]/[formMetaFn]
         * are the real uniffi bindings verbatim, and [captureFn] closes over
         * the app's one durable [CoreHolder] handle — never a fresh core
         * per capture. */
        fun create(context: Context): CaptureViewModel =
            CaptureViewModel(
                canSubmitFn = ::canSubmitCapture,
                metaProblemsFn = ::captureMetaProblems,
                formMetaFn = ::captureFormMeta,
                captureFn = { draft, nowMs ->
                    CoreHolder.get(context.applicationContext).capture(draft, nowMs)
                },
            )

        /** The factory `CaptureScreen` hands to `viewModel()`, so this
         * ViewModel is scoped to the Activity's `ViewModelStore` and not to
         * a composition. `remember { create(context) }` looked equivalent
         * and is not: `remember` survives recomposition, never Activity
         * recreation, so a rotation or a fold/unfold — the Pixel Fold is
         * the only install target — threw away the typed draft of an
         * unsubmitted capture. Being a `ViewModel` buys nothing until
         * something puts it in a store. */
        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
