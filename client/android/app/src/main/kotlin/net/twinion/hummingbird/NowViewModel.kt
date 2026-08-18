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
import uniffi.hummingbird_ffi_mobile.MobileFrontierAxis
import uniffi.hummingbird_ffi_mobile.NowBoardRecord
import uniffi.hummingbird_ffi_mobile.NowFacetSelectionRecord

/** The frontier's four facets — [uniffi.hummingbird_ffi_mobile.
 * NowFacetSelectionRecord]'s own four fields, given a name on this side of
 * the seam so [NowViewModel.toggleFacet] takes one argument instead of
 * four overloads. Mirrors `hummingbird_core::decisions::frontier::Facet`,
 * never re-exported through uniffi itself (there is nothing here for
 * Kotlin to decide with it beyond "which record field"). */
enum class FrontierFacet { CONTEXT, SIZE, ENERGY, URGENCY }

/** One facet's picked values, Kotlin's own mutable-selection shape over
 * [NowFacetSelectionRecord]'s wire `List<String>`s — a `Set` locally so
 * [NowViewModel.toggleFacet] is a plain add/remove, converted to the wire
 * record only at the [uniffi.hummingbird_ffi_mobile.MobileTaskHost.nowBoard]
 * crossing itself ([toRecord]).
 *
 * **Deliberately never persisted** — `frontier-prefs.ts`'s own rule
 * (`client/web/src/screens/frontier-prefs.ts`), ported unchanged: "You must
 * never open Now to a filtered set of columns and misread it as an empty
 * frontier." Axis and collapse state restore across a restart; the facet
 * panel always opens clear. */
data class FrontierFacetSelection(
    val context: Set<String> = emptySet(),
    val size: Set<String> = emptySet(),
    val energy: Set<String> = emptySet(),
    val urgency: Set<String> = emptySet(),
) {
    fun toRecord(): NowFacetSelectionRecord = NowFacetSelectionRecord(
        context = context.toList(),
        size = size.toList(),
        energy = energy.toList(),
        urgency = urgency.toList(),
    )

    fun count(): Int = context.size + size.size + energy.size + urgency.size

    fun toggled(facet: FrontierFacet, value: String): FrontierFacetSelection = when (facet) {
        FrontierFacet.CONTEXT -> copy(context = context.toggled(value))
        FrontierFacet.SIZE -> copy(size = size.toggled(value))
        FrontierFacet.ENERGY -> copy(energy = energy.toggled(value))
        FrontierFacet.URGENCY -> copy(urgency = urgency.toggled(value))
    }
}

private fun Set<String>.toggled(value: String): Set<String> =
    if (contains(value)) this - value else this + value

// M1-6's whole surface (#141/#504), widened to the frontier board by
// M3/#530: `NowScreen`'s read/act pair, over the same "ViewModel over
// CoreHolder, injected-fn wiring" shape `CaptureViewModel` (M1-5/#503)
// established. [fetchBoardFn] and [actFn] are the uniffi doors onto
// `MobileTaskHost.nowBoard`/`MobileTaskHost.act` verbatim in production
// ([create]) — never a re-derived ordering, grouping, urgency banding or
// affordance list on this side of the seam (the module doc on
// `hummingbird-ffi-mobile`'s `lib.rs` states why: Android calls no per-item
// decision function, only reads the already-decided [NowBoardRecord] this
// class holds). [readAxisFn]/[writeAxisFn]/[readCollapsedFn]/
// [writeCollapsedFn] are `FrontierPrefs`'s DataStore doors, injected the
// same way for the same reason: a plain JVM test can drive every method
// here without a host-architecture `.so` or a real `Context`.
class NowViewModel(
    private val fetchBoardFn: suspend (
        axis: MobileFrontierAxis,
        facets: NowFacetSelectionRecord,
        now: String,
    ) -> NowBoardRecord,
    private val actFn: suspend (itemId: String, action: String, nowMs: Long) -> Unit,
    private val readAxisFn: suspend () -> MobileFrontierAxis,
    private val writeAxisFn: suspend (MobileFrontierAxis) -> Unit,
    private val readCollapsedFn: suspend () -> Set<String>,
    private val writeCollapsedFn: suspend (Set<String>) -> Unit,
) : ViewModel() {

    private val _board = MutableStateFlow<NowBoardRecord?>(null)
    val board: StateFlow<NowBoardRecord?> = _board.asStateFlow()

    private val _loading = MutableStateFlow(true)
    val loading: StateFlow<Boolean> = _loading.asStateFlow()

    private val _axis = MutableStateFlow(MobileFrontierAxis.CONTEXT)
    val axis: StateFlow<MobileFrontierAxis> = _axis.asStateFlow()

    private val _facets = MutableStateFlow(FrontierFacetSelection())
    val facets: StateFlow<FrontierFacetSelection> = _facets.asStateFlow()

    private val _collapsed = MutableStateFlow<Set<String>>(emptySet())
    val collapsed: StateFlow<Set<String>> = _collapsed.asStateFlow()

    /** The six-card cap's "N more" toggle, per column — ephemeral like
     * [facets] (never written to [readCollapsedFn]/[writeCollapsedFn]):
     * showing more of a column is not a preference about the column, it is
     * "just this once, show me the rest". Kept in this Activity-scoped
     * ViewModel rather than a Composable's `remember` anyway, the same
     * fold/unfold-survives reasoning [NowViewModel.factory] states for the
     * whole class. */
    private val _expanded = MutableStateFlow<Set<String>>(emptySet())
    val expanded: StateFlow<Set<String>> = _expanded.asStateFlow()

    /** First load: restores the persisted axis/collapse set, then reads
     * the board once under them — `NowScreen`'s one-time `LaunchedEffect`. */
    suspend fun load(now: String) {
        _axis.value = readAxisFn()
        _collapsed.value = readCollapsedFn()
        refresh(now)
    }

    /** Reloads the board from [fetchBoardFn] under the current axis/facet
     * selection — `now` is deadline-shaped (`YYYY-MM-DDTHH:MM`), the
     * caller's own local wall clock; see `hummingbird_core::decisions::
     * urgency`'s module doc for why this crate resolves no civil date to
     * an instant itself. */
    suspend fun refresh(now: String) {
        _loading.value = true
        _board.value = fetchBoardFn(_axis.value, _facets.value.toRecord(), now)
        _loading.value = false
    }

    /** Picks a new grouping axis, persists it, and reloads under it.
     * Every column re-labels on an axis switch, so a stale collapse/expand
     * key would then apply to whatever happens to share the new name —
     * cleared with the switch, `frontier-prefs.ts`'s own reasoning
     * (`pickAxis` in `FrontierColumns.tsx`), ported. */
    suspend fun setAxis(next: MobileFrontierAxis, now: String) {
        _axis.value = next
        writeAxisFn(next)
        _collapsed.value = emptySet()
        writeCollapsedFn(emptySet())
        _expanded.value = emptySet()
        refresh(now)
    }

    /** Toggles one facet value and reloads — the facet panel is never
     * persisted (see [FrontierFacetSelection]'s own doc), so this is the
     * whole lifecycle of a pick: in memory, then straight into the next
     * [fetchBoardFn] call. */
    suspend fun toggleFacet(facet: FrontierFacet, value: String, now: String) {
        _facets.value = _facets.value.toggled(facet, value)
        refresh(now)
    }

    suspend fun clearFacets(now: String) {
        _facets.value = FrontierFacetSelection()
        refresh(now)
    }

    /** Toggles one column's collapse state and persists it — pruned first
     * against [NowBoardRecord.liveColumnKeys], the current board's own
     * pre-facet column keys for the live axis. Without pruning, the last
     * `@garden` action done, a project renamed, or a size never used again
     * leaves its collapse entry in `FrontierPrefs` forever, and a column of
     * that name reappearing later comes back collapsed for a reason the
     * reader cannot see — the exact "an override map would accrete keys
     * for panes that no longer exist" failure ADR-0021 decision 5 cites,
     * and `toggleCollapsed` in `FrontierColumns.tsx` prunes against the
     * same **pre-facet** key set for the same reason: a column the live
     * filter is merely hiding is not dead.
     *
     * `suspend`, not self-launched on `viewModelScope` —
     * `CaptureViewModel.submit`'s own reasoning, ported: the caller
     * (`NowScreen`, or a JVM test) controls the coroutine, and a JVM test
     * needs no `Dispatchers.Main` wiring to call this directly. A header
     * click is its own event (never batched with another), so writing
     * straight from the computed `next` set is safe the same way
     * `FrontierColumns.tsx` documents for its own `setCollapsed` call. */
    suspend fun toggleCollapsed(key: String) {
        // No board read yet means no known live-key set to prune
        // against — leave the collapse set exactly as it is rather than
        // treating "unknown" as "nothing is live" and wiping it.
        val liveKeys = _board.value?.liveColumnKeys?.toSet()
        val pruned = if (liveKeys != null) {
            _collapsed.value.filterTo(mutableSetOf()) { it in liveKeys }
        } else {
            _collapsed.value
        }
        val next = pruned.toggled(key)
        _collapsed.value = next
        writeCollapsedFn(next)
    }

    fun toggleExpanded(key: String) {
        _expanded.value = _expanded.value.toggled(key)
    }

    /** Applies `action` (S11/#109's wire vocabulary) to `itemId`, then
     * reloads so the row's own available-actions list (and its removal
     * from the frontier, for `complete`/`block`/`cancel`) reflects the
     * mutation immediately — local-first, the same "the overlay is
     * readable before any network is touched" criterion
     * [CaptureViewModel.submit] leans on. */
    suspend fun act(itemId: String, action: String, nowMs: Long, now: String) {
        actFn(itemId, action, nowMs)
        refresh(now)
    }

    companion object {
        /** The production wiring: every fn closes over the app's one
         * durable [CoreHolder] handle or the one [FrontierPrefs] DataStore
         * — never a fresh core or a second store per call. */
        fun create(context: Context): NowViewModel =
            NowViewModel(
                fetchBoardFn = { axis, facets, now ->
                    CoreHolder.get(context.applicationContext).nowBoard(axis, facets, now)
                },
                actFn = { itemId, action, nowMs ->
                    CoreHolder.get(context.applicationContext).act(itemId, action, nowMs)
                },
                readAxisFn = { FrontierPrefs.readAxis(context.applicationContext) },
                writeAxisFn = { axis -> FrontierPrefs.writeAxis(context.applicationContext, axis) },
                readCollapsedFn = { FrontierPrefs.readCollapsedColumns(context.applicationContext) },
                writeCollapsedFn = { collapsed ->
                    FrontierPrefs.writeCollapsedColumns(context.applicationContext, collapsed)
                },
            )

        /** The factory `NowScreen` hands to `viewModel()`, so the loaded
         * board is scoped to the Activity's `ViewModelStore` rather than to
         * a composition — the same correction [CaptureViewModel.factory]
         * documents (`remember` does not survive Activity recreation, and a
         * fold/unfold recreates). Cheaper here than there, since a lost
         * board only means a re-read rather than lost typing, but the two
         * screens holding their state the same way is the point. */
        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
