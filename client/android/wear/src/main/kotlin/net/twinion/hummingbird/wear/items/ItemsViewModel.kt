package net.twinion.hummingbird.wear.items

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.twinion.hummingbird.core.CoreHolder
import net.twinion.hummingbird.core.WallClock
import uniffi.hummingbird_ffi_mobile.MobileCalmOrder
import uniffi.hummingbird_ffi_mobile.MobileFrontierAxis
import uniffi.hummingbird_ffi_mobile.NowBoardRecord
import uniffi.hummingbird_ffi_mobile.NowFacetSelectionRecord
import uniffi.hummingbird_ffi_mobile.NowItemRecord

// The items on the wrist, by urgency (the Wear capture design handoff,
// 2026-09-10): the frontier board on its `Urgency` axis — the same
// `nowBoard` door the phone's Now screen reads, the axis the phone does not
// yet offer a button for (`HANDOFF-urgency-axis-on-android.md`) — flattened
// column by column into one list. The columns arrive in the core's order
// (`URGENCY_COLUMN_ORDER`: overdue, now, soon, calm) with each column's rows
// already ordered, so **the flattening is the order**; nothing here sorts,
// filters or re-derives a band, and `WearItemsStructuralTest` pins it. The
// watch reads: no `act`, no ack, no edit (ADR-0039's "what this does not
// decide" still stands) — an expanded row's one affordance hands the item
// to the phone.
//
// `NowItemRecord` carries what the Now card renders and no description;
// the description is `itemDetail`'s alone, so it is fetched when a row is
// opened and not before — one seam call per open, never one per row.
class ItemsViewModel(
    private val boardFn: suspend (now: String) -> NowBoardRecord,
    private val descriptionFn: suspend (itemId: String, nowMs: Long) -> String?,
) {
    /** The rows, the clock they were read at, and that clock's civil day
     * (`WallClock.todayDeadline`, the one date-of-today rule on Android) —
     * one reading, so every row's "today" is the same day. */
    data class Loaded(val rows: List<NowItemRecord>, val nowMs: Long, val today: String)

    /** The one open row: its id, and its description once fetched
     * (`null` while loading or when the item has none). */
    data class Open(val itemId: String, val description: String?, val fetched: Boolean)

    private val _loaded = MutableStateFlow<Loaded?>(null)
    /** `null` until the first load completes — a real Loading state. */
    val loaded: StateFlow<Loaded?> = _loaded.asStateFlow()

    private val _open = MutableStateFlow<Open?>(null)
    /** Session-only, one at a time: the design's single open card. */
    val open: StateFlow<Open?> = _open.asStateFlow()

    suspend fun load(nowMs: Long) {
        val board = boardFn(WallClock.local(nowMs))
        _loaded.value = Loaded(board.columns.flatMap { it.items }, nowMs, WallClock.todayDeadline(nowMs))
    }

    /** Opens [itemId] (closing whichever row was open) and fetches its
     * description; a second tap on the open row folds it. */
    suspend fun toggle(itemId: String, nowMs: Long) {
        if (_open.value?.itemId == itemId) {
            _open.value = null
            return
        }
        _open.value = Open(itemId, description = null, fetched = false)
        val description = descriptionFn(itemId, nowMs)
        // Only if this row is still the open one: a fast second tap must
        // not resurrect a folded card.
        if (_open.value?.itemId == itemId) {
            _open.value = Open(itemId, description, fetched = true)
        }
    }

    companion object {
        fun create(context: Context): ItemsViewModel {
            val app = context.applicationContext
            return ItemsViewModel(
                boardFn = { now ->
                    // No facets: the whole frontier. `MobileCalmOrder` is the
                    // calm column's own order and the seam holds no default,
                    // so it is named — the phone's choice, oldest first.
                    CoreHolder.get(app).nowBoard(
                        MobileFrontierAxis.URGENCY,
                        NowFacetSelectionRecord(emptyList(), emptyList(), emptyList(), emptyList()),
                        now,
                        MobileCalmOrder.OLDEST,
                    )
                },
                descriptionFn = { itemId, nowMs -> CoreHolder.get(app).itemDetail(itemId, nowMs)?.description },
            )
        }
    }
}
