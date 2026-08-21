package net.twinion.hummingbird

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import uniffi.hummingbird_ffi_mobile.MobileFrontierAxis
import uniffi.hummingbird_ffi_mobile.NowBoardRecord
import uniffi.hummingbird_ffi_mobile.NowColumnRecord
import uniffi.hummingbird_ffi_mobile.NowItemRecord

// The wide window's frontier board (the unfolded slice): the same columns
// the phone stacks, packed side-by-side into lanes by `FrontierLanes.kt` —
// the Kotlin port of the web's `frontier-lanes.ts`, whose module header
// carries the masonry reasoning and the ADR-0025 carve-out. A separate file
// from `NowScreen.kt` deliberately: the board is ONE item of that screen's
// one LazyColumn, and the lanes inside it are plain non-scrolling layout
// nodes, so the screen's own one-scroll pins (#537) keep reading true there.
//
// Everything drawn here is `NowScreen.kt`'s own pieces — `ColumnHeader`,
// `NowRow`, `SelectedItemCard`, `cappedColumnRows`, the "N more" toggle —
// so a column renders identically whichever width it lands on; only WHERE
// it is drawn is decided here, and that from a measured width plus rendered
// row counts, never from anything on an item (this file reads no field of
// a record beyond `id` for the selection test).

/** How wide a lane may grow — `FrontierColumns`' own card width on the web,
 * so one lone lane on a wide board holds cards, not banners. */
private val LANE_MAX_WIDTH = 380.dp

/** One column's render plan, computed once so the lane weights and the
 * drawing below cannot disagree about what is visible. */
private data class ColumnPlan(
    val column: NowColumnRecord,
    val key: String,
    val heading: String,
    val isCollapsed: Boolean,
    val isExpanded: Boolean,
    val visible: List<NowItemRecord>,
    val hidden: Int,
    val hasMoreRow: Boolean,
) {
    /** The lane-packing weight: rendered rows, not item counts — a
     * collapsed column weighs its header alone (`packLanes`' own doc). */
    val weight: Int
        get() = if (isCollapsed) 1 else 1 + visible.size + (if (hasMoreRow) 1 else 0)
}

private fun planFor(
    column: NowColumnRecord,
    axis: MobileFrontierAxis,
    collapsed: Set<String>,
    expanded: Set<String>,
    selectedId: String?,
): ColumnPlan {
    val key = column.value ?: ""
    val isCollapsed = collapsed.contains(key)
    val isExpanded = expanded.contains(key)
    val visible = when {
        isCollapsed -> emptyList()
        isExpanded -> column.items
        else -> cappedColumnRows(column.items, selectedId)
    }
    val hidden = if (isCollapsed) 0 else column.items.size - visible.size
    return ColumnPlan(
        column = column,
        key = key,
        heading = if (column.value == null) {
            NO_VALUE_LABEL[axis] ?: "No value"
        } else {
            column.label ?: "Project ${column.value}"
        },
        isCollapsed = isCollapsed,
        isExpanded = isExpanded,
        visible = visible,
        hidden = hidden,
        hasMoreRow = !isCollapsed && (hidden > 0 || (isExpanded && column.items.size > COLUMN_CAP)),
    )
}

@Composable
internal fun FrontierLaneBoard(
    board: NowBoardRecord,
    axis: MobileFrontierAxis,
    collapsed: Set<String>,
    expanded: Set<String>,
    selectedId: String?,
    dark: Boolean,
    syncTick: Int,
    /** The board container's measured width — `BoxWithConstraints`' answer,
     * null only on an unmeasured first frame (one lane per column then,
     * `laneCountFor`'s own doc). */
    boardWidthDp: Int?,
    onToggleCollapsed: (String) -> Unit,
    onToggleExpanded: (String) -> Unit,
    onSelect: (String) -> Unit,
    onComplete: (String) -> Unit,
    onCloseItem: () -> Unit,
    onGrill: (String) -> Unit,
    onMutated: () -> Unit,
    onSubmitted: () -> Unit,
) {
    val plans = board.columns.map { planFor(it, axis, collapsed, expanded, selectedId) }
    val laneCount = laneCountFor(boardWidthDp, plans.size)
    val lanes = packLanes(plans.map { it.weight }, laneCount)

    Row(horizontalArrangement = Arrangement.spacedBy(LANE_GAP_DP.dp)) {
        for (lane in lanes) {
            // `fill = false` is what gives the width cap teeth: `weight`
            // alone would fix each lane at its share, and a widthIn under a
            // fixed constraint changes nothing. Uncapped shares stay equal.
            Column(
                modifier = Modifier
                    .weight(1f, fill = false)
                    .widthIn(max = LANE_MAX_WIDTH),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                for (index in lane) {
                    LaneColumn(
                        plan = plans[index],
                        selectedId = selectedId,
                        dark = dark,
                        syncTick = syncTick,
                        onToggleCollapsed = onToggleCollapsed,
                        onToggleExpanded = onToggleExpanded,
                        onSelect = onSelect,
                        onComplete = onComplete,
                        onCloseItem = onCloseItem,
                        onGrill = onGrill,
                        onMutated = onMutated,
                        onSubmitted = onSubmitted,
                    )
                }
            }
        }
    }
}

/** One frontier column, drawn in its lane — the phone loop's own sequence
 * (header, rows with the selected one expanded in place, the "N more"
 * toggle), as plain children rather than lazy items. */
@Composable
private fun LaneColumn(
    plan: ColumnPlan,
    selectedId: String?,
    dark: Boolean,
    syncTick: Int,
    onToggleCollapsed: (String) -> Unit,
    onToggleExpanded: (String) -> Unit,
    onSelect: (String) -> Unit,
    onComplete: (String) -> Unit,
    onCloseItem: () -> Unit,
    onGrill: (String) -> Unit,
    onMutated: () -> Unit,
    onSubmitted: () -> Unit,
) {
    ColumnHeader(
        heading = plan.heading,
        count = plan.column.items.size,
        collapsed = plan.isCollapsed,
        onToggleCollapsed = { onToggleCollapsed(plan.key) },
    )
    if (!plan.isCollapsed) {
        for (record in plan.visible) {
            if (record.id == selectedId) {
                SelectedItemCard(
                    itemId = record.id,
                    syncTick = syncTick,
                    onClose = onCloseItem,
                    onGrill = onGrill,
                    onMutated = onMutated,
                    onSubmitted = onSubmitted,
                )
            } else {
                NowRow(
                    record = record.asRowModel(),
                    dark = dark,
                    selected = false,
                    onOpen = { onSelect(record.id) },
                    onComplete = { onComplete(record.id) },
                )
            }
        }
        if (plan.hasMoreRow) {
            TextButton(onClick = { onToggleExpanded(plan.key) }) {
                Text(if (plan.isExpanded) "Show fewer" else "${plan.hidden} more")
            }
        }
    }
}
