package net.twinion.hummingbird

import android.content.Context
import androidx.lifecycle.ViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.twinion.hummingbird.core.CoreHolder
import uniffi.hummingbird_ffi_mobile.NowItemRecord

// M1-6's whole surface (#141/#504): `NowScreen`'s read/act pair, over the
// same "ViewModel over CoreHolder, injected-fn wiring" shape
// `CaptureViewModel` (M1-5/#503) established. [fetchQueueFn] and [actFn] are
// the uniffi doors onto `MobileTaskHost.nowQueue`/`MobileTaskHost.act`
// verbatim in production ([create]) — never a re-derived ordering, urgency
// banding or affordance list on this side of the seam (the module doc on
// `hummingbird-ffi-mobile`'s `lib.rs` states why: Android calls no per-item
// decision function, only reads the already-decided [NowItemRecord]s this
// class holds). Injected, exactly [CaptureViewModel]'s own reasoning, so a
// plain JVM test can drive [refresh]/[act]'s control flow without a
// host-architecture `.so`.
class NowViewModel(
    private val fetchQueueFn: suspend (now: String) -> List<NowItemRecord>,
    private val actFn: suspend (itemId: String, action: String, nowMs: Long) -> Unit,
) : ViewModel() {

    private val _items = MutableStateFlow<List<NowItemRecord>>(emptyList())
    val items: StateFlow<List<NowItemRecord>> = _items.asStateFlow()

    private val _loading = MutableStateFlow(true)
    val loading: StateFlow<Boolean> = _loading.asStateFlow()

    /** Reloads the queue from [fetchQueueFn] — `now` is deadline-shaped
     * (`YYYY-MM-DDTHH:MM`), the caller's own local wall clock; see
     * `hummingbird_core::decisions::urgency`'s module doc for why this
     * crate resolves no civil date to an instant itself. */
    suspend fun refresh(now: String) {
        _loading.value = true
        _items.value = fetchQueueFn(now)
        _loading.value = false
    }

    /** Applies `action` (S11/#109's wire vocabulary) to `itemId`, then
     * reloads so the row's own available-actions list (and its removal from
     * the frontier, for `complete`/`block`/`cancel`) reflects the mutation
     * immediately — local-first, the same "the overlay is readable before
     * any network is touched" criterion [CaptureViewModel.submit] leans on. */
    suspend fun act(itemId: String, action: String, nowMs: Long, now: String) {
        actFn(itemId, action, nowMs)
        refresh(now)
    }

    companion object {
        /** The production wiring: both fns close over the app's one durable
         * [CoreHolder] handle — never a fresh core per call. */
        fun create(context: Context): NowViewModel =
            NowViewModel(
                fetchQueueFn = { now ->
                    CoreHolder.get(context.applicationContext).nowQueue(now)
                },
                actFn = { itemId, action, nowMs ->
                    CoreHolder.get(context.applicationContext).act(itemId, action, nowMs)
                },
            )
    }
}
