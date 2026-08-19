package net.twinion.hummingbird

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import java.time.LocalDateTime
import java.time.format.DateTimeFormatter
import kotlinx.coroutines.launch
import net.twinion.hummingbird.ui.theme.Amber500
import net.twinion.hummingbird.ui.theme.Crimson500
import net.twinion.hummingbird.ui.theme.Ember400
import net.twinion.hummingbird.ui.theme.Ember500
import net.twinion.hummingbird.ui.theme.UrgencyOverdueDark
import net.twinion.hummingbird.ui.theme.UrgencySoonDark
import uniffi.hummingbird_ffi_mobile.MobileFrontierAxis
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileStandingQuestion
import uniffi.hummingbird_ffi_mobile.MobileUrgencyBand
import uniffi.hummingbird_ffi_mobile.NowBlockedEntryRecord
import uniffi.hummingbird_ffi_mobile.NowItemRecord

// M1-6's whole surface (#141/#504), widened to the frontier board by
// M3/#530: the board, decided by `hummingbird-ffi-mobile::MobileTaskHost.
// nowBoard` and rendered verbatim -- this file never orders, groups, bands
// or decides an affordance itself (see `NowViewModel`'s own doc, and
// `lib.rs`'s module header for the Android-never-calls-per-item-decision-
// functions asymmetry with web this screen is the production instance of).
// The phone form is a single vertical grouped list, never a pager: the web
// board's own 390px behaviour -- its wrapping columns stacking into one
// column -- is the reference (`FrontierColumns.tsx`), so this screen is
// that one-column case made native rather than a second design. `MainActivity`'s
// `AppRoot` is this composable's host (M0's proof screen moves behind the
// "Status" action, M1-6's own scope note); no nav library in M1 --
// `onShowStatus` is a plain callback, the same mode-toggle shape a
// `NavHost` would later replace. `AppRoot` also owns the foreground sync
// cadence and hands this screen its completion via `syncTick` (see that
// parameter's own note).

/** `YYYY-MM-DDTHH:MM`, the reader's own local wall clock — the shape
 * `hummingbird_core::decisions::urgency::compute_urgency`'s module doc
 * requires from every caller, since that crate resolves no civil date to an
 * instant itself. */
private val DEADLINE_SHAPE: DateTimeFormatter = DateTimeFormatter.ofPattern("yyyy-MM-dd'T'HH:mm")

private fun nowDeadlineShaped(): String = LocalDateTime.now().format(DEADLINE_SHAPE)

/** [MobileUrgencyBand]'s dot colour, or `null` for the band that gets no
 * dot — exhaustive, no `else` arm: the compile-time drift gate the brief
 * names for a `uniffi::Enum` crossing. Values are the design mirror's
 * `--urgency-*` tokens (`.claude/skills/hummingbird-design/tokens/colors.css`),
 * light/dark split exactly `HummingbirdTheme`'s own mapping notes state the
 * pattern for.
 *
 * `CALM` maps to `null` rather than to a grey, because ADR-0021 decision 2
 * is explicit: "`calm` gets no swatch — the default is not a claim worth
 * colouring". Encoding that here rather than at the call site keeps the
 * rule with the mapping the rule is about, and keeps the `when` the one
 * place a new band has to be answered for. */
private fun urgencyColor(band: MobileUrgencyBand, dark: Boolean): Color? = when (band) {
    MobileUrgencyBand.CALM -> null
    MobileUrgencyBand.SOON -> if (dark) UrgencySoonDark else Amber500
    MobileUrgencyBand.NOW -> if (dark) Ember400 else Ember500
    MobileUrgencyBand.OVERDUE -> if (dark) UrgencyOverdueDark else Crimson500
}

/** [MobileUrgencyBand]'s mono-meta label (README: "UPPERCASE only in the
 * 11px mono meta style") — exhaustive, no `else` arm, the same discipline
 * [urgencyColor] uses.
 *
 * `CALM` maps to `null` for the same reason it takes no swatch: ADR-0021
 * decision 2 says "the default is not a claim worth colouring", and a card
 * that spends its most prominent meta slot printing "CALM" claims exactly
 * that. The word is the non-colour carrier for the three bands that *do*
 * make a claim, so only this arm goes. */
private fun urgencyLabel(band: MobileUrgencyBand): String? = when (band) {
    MobileUrgencyBand.CALM -> null
    MobileUrgencyBand.SOON -> "SOON"
    MobileUrgencyBand.NOW -> "NOW"
    MobileUrgencyBand.OVERDUE -> "OVERDUE"
}

/** S11/#109's wire vocabulary, mapped to its button label and nothing more
 * — `ItemPanel.tsx`'s `ACTION_BUTTON` verbatim (`client/web/src/components/
 * domain/ItemPanel.tsx:74`; `Mark blocked` says what `Blocked` means, per
 * the design README's voice rule; "Blocked" means an external wait and
 * nothing else). Which actions a row *offers* is decided
 * entirely core-side ([NowItemRecord.availableActions]) — this map only
 * ever renders whatever that list already contains. Shared with
 * `ItemDetailScreen`, which offers the same vocabulary from the same
 * core-decided list — two spellings of "Mark blocked" would be two
 * spellings of one domain word. */
internal val ACTION_LABEL: Map<String, String> = mapOf(
    "start" to "Start",
    "complete" to "Complete",
    "block" to "Mark blocked",
    "cancel" to "Cancel",
)

/** [NowItemRecord.stage]'s display word — `ItemRow`'s own
 * `item.stage === "ready" ? null : <StageBadge>` (web), ported: a card
 * shows its stage chip for every stage but `Ready`, which says nothing at
 * card size. `"in_progress"` still renders (`In progress`), matching web. */
private val STAGE_LABEL: Map<String, String> = mapOf(
    "triage" to "Triage",
    "grilling" to "Grilling",
    "in_progress" to "In progress",
    "blocked" to "Blocked",
    "done" to "Done",
)

/** [MobileFrontierAxis]'s switch label — `AXIS_LABEL` in
 * `FrontierColumns.tsx`, ported. */
private val AXIS_LABEL: Map<MobileFrontierAxis, String> = mapOf(
    MobileFrontierAxis.CONTEXT to "Context",
    MobileFrontierAxis.PROJECT to "Project",
    MobileFrontierAxis.SIZE to "Size",
    MobileFrontierAxis.ENERGY to "Energy",
)

/** [FRONTIER_GROUP_AXES] (`hummingbird_core::decisions::frontier`),
 * mirrored in the switch's own display order — `context` leads because it
 * is the default, exactly the core constant's own doc. */
private val FRONTIER_AXES: List<MobileFrontierAxis> = listOf(
    MobileFrontierAxis.CONTEXT,
    MobileFrontierAxis.PROJECT,
    MobileFrontierAxis.SIZE,
    MobileFrontierAxis.ENERGY,
)

/** Display text for the column of items naming no value on the live axis
 * — `NO_VALUE_LABEL` in `FrontierColumns.tsx`, ported. */
private val NO_VALUE_LABEL: Map<MobileFrontierAxis, String> = mapOf(
    MobileFrontierAxis.CONTEXT to "No context",
    MobileFrontierAxis.PROJECT to "No project",
    MobileFrontierAxis.SIZE to "No size",
    MobileFrontierAxis.ENERGY to "No energy",
)

/** `hummingbird_domain::Size`'s closed vocabulary — `ItemDetailScreen.kt`'s
 * own list (`listOf("quick", "normal", "deep")`), mirrored for the facet
 * chips rather than re-declared with different values. */
private val SIZE_VALUES = listOf("quick", "normal", "deep")

/** `hummingbird_domain::Energy`'s closed vocabulary — `ItemDetailScreen.kt`'s
 * own list, mirrored. */
private val ENERGY_VALUES = listOf("low", "medium", "high")

/** The urgency facet's closed vocabulary — `client/web/src/decisions/
 * seam.ts:387`'s own list and order (`overdue, now, soon`), ported.
 * `calm` is deliberately absent: it is the default, and "a facet for
 * 'nothing pressing' is a facet for 'everything'", which the unpicked
 * state already means. */
private val URGENCY_VALUES = listOf("overdue", "now", "soon")

/** `blockedReasonLabel` (`client/web/src/screens/blocked-reason.ts`),
 * ported verbatim: the reader of a relation-blocked row and the reader of
 * its web twin see the identical words. */
private fun blockedReasonLabel(titles: List<String>): String = when (titles.size) {
    0 -> "Blocked"
    1 -> "Blocked by: ${titles[0]}"
    2 -> "Blocked by: ${titles[0]} and ${titles[1]}"
    else -> "Blocked by: ${titles.dropLast(1).joinToString(", ")} and ${titles.last()}"
}

/** Cards shown per column before the "N more" affordance — `COLUMN_CAP` in
 * `FrontierColumns.tsx`, ported verbatim. */
private const val COLUMN_CAP = 6

/** One Now-surface pane's label, from its [MobileStandingQuestion] —
 * `StatusScreen.kt`'s own `paneLabel`, this surface's twin: a rendering
 * choice, never a decision. The Status four's arms cannot reach a
 * Now-surface list (`rank_panes(Now, ..)` never emits them, `panes::mod`'s
 * own test); named individually rather than behind a wildcard so a real
 * ninth question still trips this `when`. */
private fun nowPaneLabel(pane: MobileRankedPane): String = when (pane.standingQuestion) {
    MobileStandingQuestion.WASTE -> "Bin collection"
    MobileStandingQuestion.WEEKEND -> "This weekend"
    MobileStandingQuestion.VACATION -> "Next trip"
    MobileStandingQuestion.RACE -> "Next race — ${pane.subjectKey}"
    MobileStandingQuestion.KIMI,
    MobileStandingQuestion.GITHUB,
    MobileStandingQuestion.UPTIME,
    MobileStandingQuestion.REACHABILITY ->
        error("a Status-surface question reached the Now screen: ${pane.standingQuestion}")
}

/** Now's own standing-question panes (#537), below the queue — through the
 * same [PaneRow] shell `StatusScreen.kt` renders its own four through
 * (`PaneShell.kt`'s [rankedPaneItems]). Adds nothing while [panes] is empty
 * (the pre-first-load state, [NowViewModel.panes]'s own doc) rather than an
 * empty-state card: unlike the queue, "no panes yet" is never a fact worth
 * reporting on its own, only a moment before the first crossing lands.
 *
 * Appends into the caller's own [LazyListScope] rather than a nested
 * `Column`/`LazyColumn` of its own — the queue and the panes must share one
 * outer scroll, or a frontier taller than the viewport pushes the panes
 * section past the bottom of the screen with nothing to scroll it into view
 * (#537 review). */
private fun LazyListScope.nowPaneSection(panes: List<MobileRankedPane>) {
    if (panes.isEmpty()) return
    item(key = "panes-header") {
        Text("This week", style = MaterialTheme.typography.titleMedium)
    }
    rankedPaneItems(panes, paneLabel = ::nowPaneLabel)
}

@Composable
fun NowScreen(
    onShowStatus: () -> Unit,
    onShowAlerts: () -> Unit,
    onOpenItem: (String) -> Unit,
    syncTick: Int = 0,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    // Activity-scoped, not composition-scoped: see NowViewModel.factory.
    val viewModel: NowViewModel = viewModel(factory = NowViewModel.factory(context))
    val board by viewModel.board.collectAsState()
    val axis by viewModel.axis.collectAsState()
    val facets by viewModel.facets.collectAsState()
    val collapsed by viewModel.collapsed.collectAsState()
    val expanded by viewModel.expanded.collectAsState()
    val loading by viewModel.loading.collectAsState()
    val panes by viewModel.panes.collectAsState()
    val dark = isSystemInDarkTheme()

    suspend fun reload() {
        viewModel.refresh(nowDeadlineShaped())
        // #537: the Now surface's own panes reload alongside the queue —
        // one crossing, `MobileSurface.NOW`'s own board sibling — rather
        // than a second, independently-timed refresh cadence.
        viewModel.loadPanes(System.currentTimeMillis())
    }

    // Foreground refresh on every return to this screen — independent of
    // `syncTick` below, so a capture or an act taken elsewhere (or on
    // another device) shows up the moment this screen is looked at again,
    // even before the next sync cycle completes.
    //
    // The FIRST resume is the initial load, not a second one: a separate
    // one-shot launched effect calling `load` alongside this made entry
    // cross the seam twice, racing a default-axis board against the
    // persisted one (#530's "rendering it makes a single crossing"; the
    // structural gate names the shape that must not come back). `load` restores
    // the axis/collapse set and then refreshes, so it is the resume path's
    // own first iteration rather than a rival to it.
    LifecycleResumeEffect(Unit) {
        val resumed = scope.launch {
            if (viewModel.loadedOnce) {
                reload()
            } else {
                viewModel.load(nowDeadlineShaped())
                viewModel.loadPanes(System.currentTimeMillis())
            }
        }
        onPauseOrDispose { resumed.cancel() }
    }

    // `syncTick` is `AppRoot`'s cadence hand-off (#514 review): the
    // foreground `user`/`timer` sync legs live at the content root now, not
    // on this screen, so this is how Now learns a cycle completed — one
    // whether the tick's own cause was this screen being open or `Status`
    // being open — and re-reads `now_board` rather than rendering a stale
    // mirror until its own next resume.
    LaunchedEffect(syncTick) {
        if (syncTick > 0) reload()
    }

    fun act(itemId: String, action: String) {
        scope.launch {
            viewModel.act(itemId, action, System.currentTimeMillis(), nowDeadlineShaped())
        }
    }

    Scaffold { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                // The product name is lowercase everywhere; the screen
                // title is the one exception the design system already
                // carries (a verb/noun, not the brand).
                Text("Now", style = MaterialTheme.typography.headlineLarge)
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    TextButton(onClick = onShowAlerts) {
                        Text("Alerts")
                    }
                    TextButton(onClick = onShowStatus) {
                        Text("Status")
                    }
                }
            }

            val currentBoard = board

            AxisRow(
                axis = axis,
                onPick = { next -> scope.launch { viewModel.setAxis(next, nowDeadlineShaped()) } },
            )

            FacetFilterRow(
                facets = facets,
                // The live vocabulary the axis's own board carries
                // (`contexts_of` over the pre-facet list, `board.contexts`
                // — never a hardcoded suggested list, which would offer a
                // chip for a context nothing on the board has, or omit one
                // an item actually carries).
                contexts = currentBoard?.contexts ?: emptyList(),
                onToggle = { facet, value ->
                    scope.launch { viewModel.toggleFacet(facet, value, nowDeadlineShaped()) }
                },
                onClear = { scope.launch { viewModel.clearFacets(nowDeadlineShaped()) } },
            )

            // One LazyColumn for the whole rest of the screen — the queue
            // (whichever of its three states applies) and, appended after
            // it, the now-surface panes (#537). A second, non-scrolling
            // container after this one pushed the panes past the bottom of
            // the viewport with nothing to scroll them into view once the
            // frontier was taller than the screen (#537 review); one shared
            // scroll is the fix, not a `weight` modifier on a still-split
            // layout, since the queue's own three states already need to
            // sit inside *some* `LazyListScope` for `item`/`items` below.
            LazyColumn(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                when {
                    loading && currentBoard == null -> item(key = "loading") { CircularProgressIndicator() }
                    currentBoard == null ||
                        (currentBoard.columns.isEmpty() && currentBoard.blocked.isEmpty()) ->
                        item(key = "empty") {
                            Text(
                                // "Nothing matches what you picked" and
                                // "nothing is startable" are different facts
                                // and must not look alike
                                // (`FrontierColumns.tsx`'s own empty-result
                                // branch, ADR-0021 decision 5's whole reason
                                // for never persisting the filter): a
                                // facet-emptied board says so, rather than
                                // reporting an empty frontier it is not.
                                if (facets.count() > 0) {
                                    "Nothing matches what you picked."
                                } else {
                                    // Honesty over reassurance (README): an
                                    // empty frontier is reported as a fact,
                                    // not apologised for.
                                    "Nothing on the frontier."
                                },
                                style = MaterialTheme.typography.bodyLarge,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    else -> {
                        for (column in currentBoard.columns) {
                            val key = column.value ?: ""
                            val isCollapsed = collapsed.contains(key)
                            val heading = if (column.value == null) {
                                NO_VALUE_LABEL[axis] ?: "No value"
                            } else {
                                column.label ?: "Project ${column.value}"
                            }

                            item(key = "header-$key") {
                                ColumnHeader(
                                    heading = heading,
                                    count = column.items.size,
                                    collapsed = isCollapsed,
                                    onToggleCollapsed = {
                                        scope.launch { viewModel.toggleCollapsed(key) }
                                    },
                                )
                            }

                            if (!isCollapsed) {
                                val isExpanded = expanded.contains(key)
                                val visible = if (isExpanded) column.items else column.items.take(COLUMN_CAP)
                                val hidden = column.items.size - visible.size

                                items(visible, key = { "$key-${it.id}" }) { record ->
                                    NowRow(
                                        record = record,
                                        dark = dark,
                                        onOpen = { onOpenItem(record.id) },
                                        onAct = { action -> act(record.id, action) },
                                    )
                                }

                                if (hidden > 0 || (isExpanded && column.items.size > COLUMN_CAP)) {
                                    item(key = "more-$key") {
                                        TextButton(onClick = { viewModel.toggleExpanded(key) }) {
                                            Text(if (isExpanded) "Show fewer" else "$hidden more")
                                        }
                                    }
                                }
                            }
                        }

                        if (currentBoard.blocked.isNotEmpty()) {
                            item(key = "blocked-header") {
                                ColumnHeader(
                                    heading = "Blocked",
                                    count = currentBoard.blocked.size,
                                    collapsed = false,
                                    onToggleCollapsed = null,
                                )
                            }
                            items(currentBoard.blocked, key = { "blocked-${it.item.id}" }) { entry ->
                                BlockedRow(
                                    entry = entry,
                                    dark = dark,
                                    onOpen = { onOpenItem(entry.item.id) },
                                    onAct = { action -> act(entry.item.id, action) },
                                )
                            }
                        }
                    }
                }

                // #537: the now-surface panes — waste/weekend/vacation/race
                // — below the queue, the same placement the web's own aside
                // stacks into at 390px (this issue's own "the parity
                // reference, not a simplification" line). Appended whatever
                // the queue's own state above was (loading, empty,
                // populated): the panes are a separate crossing
                // ([NowViewModel.loadPanes]), never gated on the board's.
                nowPaneSection(panes)
            }
        }
    }
}

/** The axis switch — every grouping axis, in [FRONTIER_AXES]'s order. Never
 * decides which axis groups what; picking one only tells [NowViewModel]
 * which already-decided board to ask for next. */
@Composable
private fun AxisRow(axis: MobileFrontierAxis, onPick: (MobileFrontierAxis) -> Unit) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        for (candidate in FRONTIER_AXES) {
            FilterChip(
                selected = axis == candidate,
                onClick = { onPick(candidate) },
                label = { Text(AXIS_LABEL[candidate] ?: candidate.name) },
            )
        }
    }
}

/** The facet filter — one chip group per facet. Size/Energy/Urgency read
 * from the closed vocabulary lists above; Context reads [contexts] —
 * [uniffi.hummingbird_ffi_mobile.NowBoardRecord.contexts], the live
 * vocabulary `frontier::contexts_of` decided over the current (pre-facet)
 * board, never a hardcoded suggested list. Selection lives in
 * [NowViewModel]'s in-memory [FrontierFacetSelection] and is never
 * persisted (that class's own doc has the reason); toggling only ever
 * asks for a fresh, already-decided board. */
@Composable
private fun FacetFilterRow(
    facets: FrontierFacetSelection,
    contexts: List<String>,
    onToggle: (FrontierFacet, String) -> Unit,
    onClear: () -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        FacetChipGroup("Context", FrontierFacet.CONTEXT, contexts, facets.context, onToggle)
        FacetChipGroup("Size", FrontierFacet.SIZE, SIZE_VALUES, facets.size, onToggle)
        FacetChipGroup("Energy", FrontierFacet.ENERGY, ENERGY_VALUES, facets.energy, onToggle)
        FacetChipGroup("Urgency", FrontierFacet.URGENCY, URGENCY_VALUES, facets.urgency, onToggle)
        if (facets.count() > 0) {
            TextButton(onClick = onClear) { Text("Clear filters") }
        }
    }
}

@Composable
private fun FacetChipGroup(
    label: String,
    facet: FrontierFacet,
    values: List<String>,
    selected: Set<String>,
    onToggle: (FrontierFacet, String) -> Unit,
) {
    // `FacetRow` in `FrontierColumns.tsx` returns nothing for an empty
    // vocabulary (`values.length === 0`) — a board with, say, nothing on
    // it yet offers no Context chips at all rather than a label over an
    // empty row.
    if (values.isEmpty()) return

    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(
            label,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            for (value in values) {
                FilterChip(
                    selected = selected.contains(value),
                    onClick = { onToggle(facet, value) },
                    label = { Text(value) },
                )
            }
        }
    }
}

/** One column's (or the Blocked section's) header: its heading, its own
 * count (visible even while collapsed — a closed column must still say
 * how much is inside it), and the collapse toggle. `onToggleCollapsed`
 * is `null` for a section with no collapse of its own (Blocked).
 *
 * The whole row is the toggle, so it carries the design system's 44dp
 * minimum touch target (README: "a 44px row height that doubles as the
 * minimum touch target on every surface") — a `clickable` modifier, unlike
 * Material3's own control components, enforces no minimum of its own, and
 * a `titleMedium` line alone is well under it.
 *
 * The state is drawn with [R.drawable.ic_chevron_down], rotated a
 * quarter-turn when the column is shut, not with a Unicode triangle: the
 * design system's ICONOGRAPHY rule is "Unicode as icons: never". A section
 * with no toggle draws no mark at all rather than a disabled-looking one. */
@Composable
private fun ColumnHeader(
    heading: String,
    count: Int,
    collapsed: Boolean,
    onToggleCollapsed: (() -> Unit)?,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(
                if (onToggleCollapsed != null) {
                    Modifier.heightIn(min = 44.dp).clickable(onClick = onToggleCollapsed)
                } else {
                    Modifier
                },
            ),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        if (onToggleCollapsed != null) {
            Icon(
                painterResource(R.drawable.ic_chevron_down),
                // The heading says which column; this mark says only
                // whether it is open, so that is what it names.
                contentDescription = if (collapsed) "Expand" else "Collapse",
                modifier = Modifier
                    .size(20.dp)
                    .rotate(if (collapsed) -90f else 0f),
            )
        }
        Text(
            heading,
            style = MaterialTheme.typography.titleMedium,
            modifier = Modifier.weight(1f).padding(start = if (onToggleCollapsed != null) 4.dp else 0.dp),
        )
        Text(
            "$count",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** A relation-blocked row: [NowRow] verbatim, dimmed, with its blocked
 * reason underneath — `NowScreen.tsx`'s own wrapper (opacity 0.6, never a
 * second dimming source stacked on `pending`'s own chip). */
@Composable
private fun BlockedRow(
    entry: NowBlockedEntryRecord,
    dark: Boolean,
    onOpen: () -> Unit,
    onAct: (String) -> Unit,
) {
    Column(
        modifier = Modifier.alpha(0.6f),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        NowRow(record = entry.item, dark = dark, onOpen = onOpen, onAct = onAct)
        Text(
            blockedReasonLabel(entry.blockedByTitles),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.error,
            modifier = Modifier.padding(start = 16.dp, end = 16.dp),
        )
    }
}

@Composable
private fun NowRow(
    record: NowItemRecord,
    dark: Boolean,
    onOpen: () -> Unit,
    onAct: (String) -> Unit,
) {
    // The card itself is the door to item detail (#521). Material3's
    // `onClick` overload rather than a `clickable` modifier: it carries the
    // ripple, the `role = Button` semantics and the minimum touch target
    // that a bare modifier leaves to the caller. No chevron and no other
    // added chrome -- the design system's `interactive` card is the whole
    // affordance, and its icon vocabulary has no chevron in it.
    //
    // The action buttons below stay reachable: a nested clickable consumes
    // its own press, so `Complete` still completes rather than opening.
    Card(
        onClick = onOpen,
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            // Every entry on the meta row is conditional -- the swatch and the
            // word both go for `calm`, the deadline draws only when set, and
            // `ready` says nothing -- so the ordinary minted action reaches
            // this row with nothing to put on it. An empty `Row` is not free:
            // it measures zero high but the Column's `spacedBy(8.dp)` still
            // pays for it, stranding 8dp above the title. Read once here and
            // reused below, so the guard cannot disagree with what draws.
            val swatch = urgencyColor(record.urgency, dark)
            val urgencyWord = urgencyLabel(record.urgency)
            val stageChip = if (record.stage == "ready") null else STAGE_LABEL[record.stage] ?: record.stage
            if (swatch != null || urgencyWord != null || record.deadline != null || stageChip != null) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    swatch?.let {
                        Box(
                            modifier = Modifier
                                .size(8.dp)
                                .background(it, CircleShape),
                        )
                    }
                    urgencyWord?.let { label ->
                        Text(
                            label,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    record.deadline?.let {
                        Text(
                            it,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    // The stage chip IS the triage label: a capture riding
                    // inline into these columns is marked by the app's one
                    // stage vocabulary, never a badge invented for this
                    // surface (`ItemRow`'s own rule, ported). "Ready" says
                    // nothing at card size and is the default, so it alone
                    // renders no chip.
                    stageChip?.let {
                        Text(
                            it,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }

            Text(record.title, style = MaterialTheme.typography.bodyLarge)

            if (record.availableActions.isNotEmpty()) {
                // A `FlowRow`, not a `Row`: a `Ready` item offers all four
                // actions (`decisions::actions::available_actions`), and
                // "Start / Complete / Mark blocked / Cancel" is wider than
                // a phone card — a fixed, non-scrolling Row clipped the
                // trailing action on the ordinary case, not just on the
                // Fold's cover display. Wrapping keeps every offered action
                // reachable at any width without this file deciding which
                // ones matter.
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    for (action in record.availableActions) {
                        OutlinedButton(onClick = { onAct(action) }) {
                            Text(ACTION_LABEL[action] ?: action)
                        }
                    }
                }
            }
        }
    }
}
