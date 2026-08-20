package net.twinion.hummingbird

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.twinion.hummingbird.core.CoreHolder
import net.twinion.hummingbird.core.ZoneBridge
import uniffi.hummingbird_ffi_mobile.MobileFrontierAxis
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileSurface
import uniffi.hummingbird_ffi_mobile.MobileSyncFacts
import uniffi.hummingbird_ffi_mobile.MobileZoneFact
import uniffi.hummingbird_ffi_mobile.MobileZoneQuery
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
// M3/#530: `NowScreen`'s board reader, over the same "ViewModel over
// CoreHolder, injected-fn wiring" shape `CaptureViewModel` (M1-5/#503)
// established. [fetchBoardFn] is the uniffi door onto
// `MobileTaskHost.nowBoard` verbatim in production
// ([create]) — never a re-derived ordering, grouping, urgency banding or
// affordance list on this side of the seam (the module doc on
// `hummingbird-ffi-mobile`'s `lib.rs` states why: Android calls no per-item
// decision function, only reads the already-decided [NowBoardRecord] this
// class holds). This class carries exactly one act door — [complete], the
// row checkmark's verb, brought back on operator feedback 2026-08-19 —
// while the wider act vocabulary stays with `ItemDetailViewModel` behind
// the opened item; `NowScreenStructuralTest` pins the door to that one
// verb. [readAxisFn]/[writeAxisFn]/[readCollapsedFn]/
// [writeCollapsedFn] are `FrontierPrefs`'s DataStore doors, injected the
// same way for the same reason: a plain JVM test can drive every method
// here without a host-architecture `.so` or a real `Context`.
class NowViewModel(
    private val fetchBoardFn: suspend (
        axis: MobileFrontierAxis,
        facets: NowFacetSelectionRecord,
        now: String,
    ) -> NowBoardRecord,
    private val readAxisFn: suspend () -> MobileFrontierAxis,
    private val writeAxisFn: suspend (MobileFrontierAxis) -> Unit,
    private val readCollapsedFn: suspend () -> Set<String>,
    private val writeCollapsedFn: suspend (Set<String>) -> Unit,
    // #537's standing-question panes (waste/weekend/vacation/race), below
    // the queue: `paneZoneQueriesFn`/`rankPanesFn` are the pane lane's own
    // two-phase zone bridge (`MobileTaskHost.paneZoneQueries`/`.rankPanes`
    // in production, `ZoneBridge.resolve` the host-side resolve leg between
    // them); `setScheduledDateFn` is the weekend pane's do-date write
    // (`MobileTaskHost.setScheduledDate`) — see that method's own doc for
    // why it is a dedicated seam call rather than a full `ItemEdit`.
    private val paneZoneQueriesFn: suspend (nowMs: Long) -> List<MobileZoneQuery>,
    private val rankPanesFn: suspend (nowMs: Long, zoneFacts: List<MobileZoneFact>) -> List<MobileRankedPane>,
    private val setScheduledDateFn: suspend (itemId: String, date: String?, nowMs: Long) -> Unit,
    private val completeFn: suspend (itemId: String, nowMs: Long) -> Unit,
) : ViewModel() {

    private val _board = MutableStateFlow<NowBoardRecord?>(null)
    val board: StateFlow<NowBoardRecord?> = _board.asStateFlow()

    /** The Now surface's own three-or-four panes (#537) — empty until
     * [loadPanes] first returns, exactly [board]'s own "no crossing has
     * landed yet" reading (never a distinct loading flag here: the panes
     * section renders nothing while empty, the same way `NowScreen`'s queue
     * itself waits on [loading] rather than guessing). */
    private val _panes = MutableStateFlow<List<MobileRankedPane>>(emptyList())
    val panes: StateFlow<List<MobileRankedPane>> = _panes.asStateFlow()

    private val _loading = MutableStateFlow(true)
    val loading: StateFlow<Boolean> = _loading.asStateFlow()

    private val _axis = MutableStateFlow(MobileFrontierAxis.CONTEXT)
    val axis: StateFlow<MobileFrontierAxis> = _axis.asStateFlow()

    private val _facets = MutableStateFlow(FrontierFacetSelection())
    val facets: StateFlow<FrontierFacetSelection> = _facets.asStateFlow()

    /** The facet panel's disclosure — shut by default, filtering is the
     * occasional gesture (`FrontierColumns.tsx`'s own reasoning: only the
     * axis switch earns permanent space). Ephemeral like [facets] itself
     * and never persisted, but Activity-scoped here rather than a
     * Composable `remember`, the same fold/unfold reasoning [factory]
     * states for the whole class. */
    private val _filtersOpen = MutableStateFlow(false)
    val filtersOpen: StateFlow<Boolean> = _filtersOpen.asStateFlow()

    fun toggleFiltersOpen() {
        _filtersOpen.value = !_filtersOpen.value
    }

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

    /** Now's inline expansion: which item's panel stands above the board —
     * `TriageViewModel`'s one-open-at-a-time shape, tap-again-to-collapse.
     * Ephemeral view state like [expanded], Activity-scoped for the same
     * fold/unfold reason, and never persisted: reopening the app onto a
     * days-old expansion would claim a currency the selection no longer
     * has. */
    private val _selectedItemId = MutableStateFlow<String?>(null)
    val selectedItemId: StateFlow<String?> = _selectedItemId.asStateFlow()

    fun selectItem(itemId: String) {
        _selectedItemId.value = if (_selectedItemId.value == itemId) null else itemId
    }

    fun closeItem() {
        _selectedItemId.value = null
    }

    /** The one failure line this screen owns — set only by [complete],
     * cleared on its next attempt; `TriageViewModel`'s `statusLine`
     * shape. */
    private val _statusLine = MutableStateFlow<String?>(null)
    val statusLine: StateFlow<String?> = _statusLine.asStateFlow()

    /** The row checkmark (the web `ItemRow`'s `MarkDoneButton`):
     * `Core::act`'s `complete`, then a board re-read so the row leaves the
     * frontier in the same gesture — `TriageViewModel.complete`'s idiom,
     * selection included: a completed row's expanded panel must not stay
     * standing over a board that no longer holds it, so it closes on
     * success — and only on success: a failed act leaves the row on the
     * board, panel standing, with [statusLine] read against it.
     * Cancellation rethrows rather than being worded as a failure. */
    suspend fun complete(itemId: String, now: String, nowMs: Long) {
        _statusLine.value = null
        val failure = try {
            completeFn(itemId, nowMs)
            null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            "Couldn't complete — ${error.message}"
        }
        if (failure == null && _selectedItemId.value == itemId) _selectedItemId.value = null
        refresh(now)
        failure?.let { _statusLine.value = it }
    }

    /** Whether [load] has completed at least once on this (Activity-scoped)
     * instance — `NowScreen`'s resume effect reads it to tell its first
     * resume, which must restore the persisted axis/collapse set, from
     * every later one, which must not re-read preferences already held
     * here. Set only after [load] returns, so a resume cancelled mid-load
     * (a fold, a fast Back) leaves the next one to do the load again. */
    var loadedOnce: Boolean = false
        private set

    /** First load: restores the persisted axis/collapse set, then reads
     * the board once under them — `NowScreen`'s first resume. */
    suspend fun load(now: String) {
        _axis.value = readAxisFn()
        _collapsed.value = readCollapsedFn()
        refresh(now)
        loadedOnce = true
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

    /** #537's pane load: the zone bridge's two phases, back to back — every
     * `(zone, civil-date)` fact the Now surface's sunk questions need
     * ([paneZoneQueriesFn]), resolved by the host
     * ([net.twinion.hummingbird.core.ZoneBridge], in production), then
     * ranked against those resolved facts ([rankPanesFn]). Never persisted
     * and never merged with the previous list — a reload replaces it whole,
     * [StatusViewModel.load]'s own shape. */
    suspend fun loadPanes(nowMs: Long) {
        val queries = paneZoneQueriesFn(nowMs)
        val facts = ZoneBridge.resolve(queries)
        _panes.value = rankPanesFn(nowMs, facts)
    }

    /** The weekend-plans pane's do-date chip (#537, #122): writes through
     * [setScheduledDateFn] — [uniffi.hummingbird_ffi_mobile.MobileTaskHost.
     * setScheduledDate], the seam mutation wrapping `Core::triage` with no
     * destination change — then reloads the panes so the pane's own band
     * reflects the write immediately — local-first, the same "the overlay
     * is readable before any network is touched" criterion
     * [CaptureViewModel.submit] leans on. `date == null` clears an
     * already-planned day. */
    suspend fun setScheduledDate(itemId: String, date: String?, nowMs: Long) {
        setScheduledDateFn(itemId, date, nowMs)
        loadPanes(nowMs)
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
                readAxisFn = { FrontierPrefs.readAxis(context.applicationContext) },
                writeAxisFn = { axis -> FrontierPrefs.writeAxis(context.applicationContext, axis) },
                readCollapsedFn = { FrontierPrefs.readCollapsedColumns(context.applicationContext) },
                writeCollapsedFn = { collapsed ->
                    FrontierPrefs.writeCollapsedColumns(context.applicationContext, collapsed)
                },
                paneZoneQueriesFn = { nowMs ->
                    CoreHolder.get(context.applicationContext).paneZoneQueries(MobileSurface.NOW, nowMs)
                },
                rankPanesFn = { nowMs, zoneFacts ->
                    CoreHolder.get(context.applicationContext).rankPanes(
                        MobileSurface.NOW,
                        nowMs,
                        zoneFacts,
                        // No sync history to fold in: the reachability pane
                        // is the only sunk reader of it, and it never sinks
                        // into `Surface::Now` (`panes::mod::SUNK`).
                        MobileSyncFacts(null, null, null),
                    )
                },
                setScheduledDateFn = { itemId, date, nowMs ->
                    CoreHolder.get(context.applicationContext).setScheduledDate(itemId, date, nowMs)
                },
                completeFn = { itemId, nowMs ->
                    CoreHolder.get(context.applicationContext).act(itemId, "complete", nowMs)
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
