package net.twinion.hummingbird

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.VectorConverter
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import kotlin.math.abs
import kotlinx.coroutines.launch
import net.twinion.hummingbird.ui.theme.CarryTiltMaxDegrees
import net.twinion.hummingbird.ui.theme.CarryTiltPerPixelPerSecond
import net.twinion.hummingbird.ui.theme.carrySpring
import net.twinion.hummingbird.ui.theme.reducedMotion
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
//
// **Since #801 a card is also carried between columns here** (ADR-0021
// decision 9), on the wide window alone: the phone stacks its columns down
// one list, where a drag between them would be a scroll through the board
// rather than a movement across it. What a drop writes is decided in the
// core and reached through two seam doors — `droppableColumns`, asked once
// when a gesture starts so a refused column is never lit, and
// `moveItemToColumn`, called once when it ends. Nothing about that mapping
// is expressed in Kotlin. What IS here is the gesture: a long press, a card
// on a spring, the column under it lit, and a release that either commits
// or springs the card home.

/** How wide a lane may grow — `FrontierColumns`' own card width on the web,
 * so one lone lane on a wide board holds cards, not banners. */
private val LANE_MAX_WIDTH = 380.dp

/** `--radius-card`, the same 14dp every card on this surface wears — the
 * lit column's own outline follows it so the tint reads as one surface
 * rather than as a box drawn around cards. */
private val COLUMN_SHAPE = RoundedCornerShape(14.dp)

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
    /** Which of `columnKeys` this card could land in — the core's answer
     * (`MobileTaskHost.droppableColumns`), asked once per gesture so a
     * column that would refuse the card is never lit. */
    onDroppableColumns: suspend (itemId: String, columnKeys: List<String>) -> List<String> =
        { _, _ -> emptyList() },
    /** Commit a drop (`MobileTaskHost.moveItemToColumn`). The empty string
     * is the no-value column, exactly as `NowColumnRecord.value` carries
     * it. */
    onMoveItem: (itemId: String, targetColumnKey: String) -> Unit = { _, _ -> },
) {
    val plans = board.columns.map { planFor(it, axis, collapsed, expanded, selectedId) }
    val laneCount = laneCountFor(boardWidthDp, plans.size)
    val lanes = packLanes(plans.map { it.weight }, laneCount)
    val drag = rememberBoardDrag(board, plans.map { it.key }, onDroppableColumns, onMoveItem)

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
                        drag = drag,
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

/** The board's live gesture, in one place because only one card is ever
 * carried at a time.
 *
 * Everything here is geometry and motion. Which column a drop may land in
 * is [droppable]'s answer, fetched once from the core when a gesture starts
 * and held for its life — a per-frame crossing is not affordable, and a
 * predicate Kotlin evaluated itself would be the duplication ADR-0025
 * exists to prevent.
 *
 * Column boxes are collected by [bounds] on every layout and **snapshotted
 * at drag start**, for the reason the web freezes its own column rects: the
 * lane packing must not move the target out from under the finger. It
 * cannot here either — the packing weights are resting heights and nothing
 * changes them mid-drag — so the snapshot is belt and braces rather than
 * the load-bearing part it is on the web. */
private class BoardDrag(
    val offset: Animatable<Offset, *>,
    val bounds: MutableMap<String, Rect>,
    private val frozen: MutableMap<String, Rect>,
    private val gentle: Boolean,
    private val onDroppableColumns: suspend (String, List<String>) -> List<String>,
    private val onMoveItem: (String, String) -> Unit,
    private val launch: (suspend () -> Unit) -> Unit,
) {
    /** The card being carried, and where it was when the gesture started.
     * Observable: the carried card's own transform is read while composing,
     * so starting a gesture has to recompose the board once. */
    var carrying: String? by mutableStateOf(null)
    private var carriedFrom: Rect? = null
    private var sourceKey: String? = null

    /** Where each card was last laid out, so a gesture that starts on one
     * knows what it is moving. Read at the long press and never after. */
    private val cardBounds = mutableMapOf<String, Rect>()

    /** Every column the board is currently drawing, refilled on each
     * composition so the one crossing a gesture makes can ask about all of
     * them at once. */
    var columnKeys: List<String> = emptyList()

    /** The column key under the card, or `null` when it is over nothing the
     * card may land in. Read by every column to decide whether it is lit. */
    var over: String? by mutableStateOf(null)

    /** The columns the core says this card may land in — empty until the
     * one crossing per gesture returns, so nothing lights for a frame or
     * two rather than lighting something that would then refuse. */
    var allowed: List<String> = emptyList()

    fun start(itemId: String, columnKey: String, from: Rect) {
        carrying = itemId
        carriedFrom = from
        sourceKey = columnKey
        over = null
        allowed = emptyList()
        frozen.clear()
        frozen.putAll(bounds)
        launch {
            val answer = onDroppableColumns(itemId, columnKeys)
            // A gesture that ended while the crossing was in flight must
            // not have its answer applied to the next one.
            if (carrying == itemId) allowed = answer
        }
    }

    fun drag(amount: Offset) {
        val target = offset.value + amount
        over = columnUnder(target)
        launch { if (gentle) offset.snapTo(target) else offset.animateTo(target, carrySpring()) }
    }

    fun end() {
        val itemId = carrying
        val landing = over
        carrying = null
        carriedFrom = null
        sourceKey = null
        over = null
        allowed = emptyList()
        if (itemId != null && landing != null) {
            // The board reloads on the write, and the recomposed card is
            // drawn in its new column at rest — so the offset is dropped
            // here rather than animated into a slot. No FLIP on Android:
            // the web's is what pays for holding a card above an
            // asynchronous re-read, and this board re-reads on the same
            // gesture the finger ended.
            onMoveItem(itemId, landing)
        }
        // Refused, or released over nothing: the card springs home, which
        // is the same motion as landing and needs no separate branch.
        launch { if (gentle) offset.snapTo(Offset.Zero) else offset.animateTo(Offset.Zero, carrySpring()) }
    }

    private fun columnUnder(at: Offset): String? {
        val from = carriedFrom ?: return null
        val centre = from.center + at
        val landing = frozen.entries
            .firstOrNull { (_, rect) -> rect.contains(centre) }
            ?.key
            ?: return null
        return if (landing == sourceKey || landing !in allowed) null else landing
    }

    /** What one card wears: the long press that starts a gesture, and —
     * while it is the card being carried — the transform that carries it
     * and the lift that puts it above the board it is crossing.
     *
     * `onGloballyPositioned` sits OUTSIDE `graphicsLayer` in the chain on
     * purpose, so the box it reports is where the card is laid out rather
     * than where it is currently drawn. */
    fun cardModifier(itemId: String, columnKey: String): Modifier =
        Modifier
            .onGloballyPositioned { cardBounds[itemId] = it.boundsInWindow() }
            .pointerInput(itemId, columnKey) {
                detectDragGesturesAfterLongPress(
                    onDragStart = {
                        start(itemId, columnKey, cardBounds[itemId] ?: Rect.Zero)
                    },
                    onDrag = { change, amount ->
                        change.consume()
                        drag(amount)
                    },
                    onDragEnd = { end() },
                    onDragCancel = { end() },
                )
            }
            .then(
                if (carrying != itemId) {
                    Modifier
                } else {
                    Modifier
                        .zIndex(1f)
                        .graphicsLayer {
                            translationX = offset.value.x
                            translationY = offset.value.y
                            // Lean into the travel, and right itself as the
                            // spring settles — the spring's own velocity is
                            // what makes that read as weight rather than as
                            // a fixed tilt.
                            rotationZ = (offset.velocity.x * CarryTiltPerPixelPerSecond)
                                .coerceIn(-CarryTiltMaxDegrees, CarryTiltMaxDegrees)
                            shadowElevation = CARRIED_ELEVATION
                        }
                },
            )
}

/** The floating step — what a dialog gets, which is what a card held above
 * the board is. */
private const val CARRIED_ELEVATION = 12f

/** [BoardDrag] for this composition, reset whenever a new board arrives —
 * a reload re-reads every column, so a gesture's frozen geometry and its
 * held offset belong to the board it started on. */
@Composable
private fun rememberBoardDrag(
    board: NowBoardRecord,
    columnKeys: List<String>,
    onDroppableColumns: suspend (String, List<String>) -> List<String>,
    onMoveItem: (String, String) -> Unit,
): BoardDrag {
    val scope = rememberCoroutineScope()
    val offset = remember { Animatable(Offset.Zero, Offset.VectorConverter) }
    val bounds = remember { mutableMapOf<String, Rect>() }
    val frozen = remember { mutableMapOf<String, Rect>() }
    val gentle = reducedMotion()
    val drag = remember(gentle) {
        BoardDrag(
            offset = offset,
            bounds = bounds,
            frozen = frozen,
            gentle = gentle,
            onDroppableColumns = onDroppableColumns,
            onMoveItem = onMoveItem,
            launch = { block -> scope.launch { block() } },
        )
    }
    drag.columnKeys = columnKeys
    LaunchedEffect(board) {
        drag.carrying = null
        drag.over = null
        if (abs(offset.value.x) > 0f || abs(offset.value.y) > 0f) offset.snapTo(Offset.Zero)
    }
    return drag
}

/** One frontier column, drawn in its lane — the phone loop's own sequence
 * (header, rows with the selected one expanded in place, the "N more"
 * toggle).
 *
 * A `Column` of its own since #801, where the phone loop emits its children
 * straight into the lane: a drop target needs one node to measure and one
 * surface to tint. The lane's own `spacedBy(8.dp)` is repeated inside so
 * the spacing a reader sees is unchanged by the wrapping. */
@Composable
private fun LaneColumn(
    plan: ColumnPlan,
    drag: BoardDrag,
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
    val lit = drag.over == plan.key
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .onGloballyPositioned { drag.bounds[plan.key] = it.boundsInWindow() }
            // A lit column gets more solid, never less: `--surface-quiet`
            // and `--border-strong`, the design system's own hover rule,
            // and the same treatment the web board paints. A column that
            // would REFUSE the card is simply never lit — no red anywhere,
            // since colour on this board means urgency.
            .then(
                if (lit) {
                    Modifier
                        .background(MaterialTheme.colorScheme.surfaceVariant, COLUMN_SHAPE)
                        .border(1.dp, MaterialTheme.colorScheme.outline, COLUMN_SHAPE)
                        .padding(4.dp)
                } else {
                    Modifier
                },
            ),
        verticalArrangement = Arrangement.spacedBy(8.dp),
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
                        modifier = drag.cardModifier(record.id, plan.key),
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
}
