package net.twinion.hummingbird.ui.panes

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import java.time.ZoneId
import java.util.Locale
import net.twinion.hummingbird.brand.R
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswerState
import uniffi.hummingbird_ffi_mobile.MobileHomeworkFacts
import uniffi.hummingbird_ffi_mobile.MobileHomeworkItem
import uniffi.hummingbird_ffi_mobile.MobileHomeworkResolved
import uniffi.hummingbird_ffi_mobile.MobilePaneFacts
import uniffi.hummingbird_ffi_mobile.MobileRaceFacts
import uniffi.hummingbird_ffi_mobile.MobileRaceResolved
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileScpsEvent
import uniffi.hummingbird_ffi_mobile.MobileScpsResolved
import uniffi.hummingbird_ffi_mobile.MobileWasteResolved
import uniffi.hummingbird_ffi_mobile.MobileWasteStream
import uniffi.hummingbird_ffi_mobile.MobileWeekendEntry
import uniffi.hummingbird_ffi_mobile.MobileWeekendResolved

// The Now surface's expanded renderings (the pane-content slice, second
// half) — homework (#675), waste and race, each web `*PaneExpanded.tsx`
// ported: the bins do
// the talking on waste (real kerb colours, three words, a date), and the
// race card is the prototype's series tile under real rules — plus, since
// #564/#621, the weekend card. Its entries are `weekend.rs`'s own merged
// `days` (sunk at #564 precisely so this card and `WeekendPaneExpanded.tsx`
// cannot disagree about the due-beats-scheduled dedupe), and its plan chips
// are `MobileTaskHost.setScheduledDate`'s first caller. That list holds
// only the days that have not yet ended at the device — it shrinks to
// Sunday alone by Sunday morning — and this card takes BOTH its day
// sections and its `dayKeys` (the plan chips' own set) straight off it, so
// the shrink is inherited rather than re-decided here. `weekend.rs`'s
// `WeekendWindow::days` is where that rule lives.
//
// **Vacation still has no card**, and that is a scope line rather than a
// missing lane now: the trips themselves cross fine, but `MobileTrip`
// carries no event title, so a card here would name every trip by its
// location or "a trip" — see `PaneAnswers.kt`'s `vacationTripHeadline` for
// the same recorded divergence. The shell's collapsed headline is the
// honest whole story until the seam grows the name the web derives.
//
// **SCPS (#694) got its own card**, `ScpsPaneExpanded.tsx` ported: the next
// event (kind, time, location, notes), the Photo Quest line, and any
// further `SCPS `-titled events beneath. The card TITLE carries kind and
// topic only (`scpsCardTitle`, ported); the day and time live on a separate
// meta line beneath it — `ScpsPaneExpanded.tsx`'s own rule (restated from
// `VacationPaneExpanded.tsx`): a title repeating the meta line's facts
// would say the same thing twice.
//
// **Nothing here decides.** Bands, answer states and facts arrive on the
// pane; the words reuse `PaneAnswers.kt`'s ports (`wasteCollapsedHeadline`
// is deliberately NOT reused for the big line — the web's expanded
// headline is its own, wordier sentence, ported below). One recorded gap:
// the web's race card shows the live alert's TITLE (`view.liveAlert.title`);
// the mobile mirror carries only `hasLiveAlert`, so this card says
// "starting soon" without the title.
//
// Exhaustive `when`s with no `else` arm throughout — the house drift gate.

/** The Now surface's `expandedContent` — one dispatcher, exhaustive over
 * every facts arm the way `NowScreen.kt`'s `nowPaneLabel` is. */
@Composable
internal fun NowPaneExpanded(
    pane: MobileRankedPane,
    nowMs: Long,
    onSetScheduledDate: (itemId: String, date: String?) -> Unit = { _, _ -> },
) {
    when (val facts = pane.facts) {
        is MobilePaneFacts.Homework -> HomeworkPaneExpanded(facts.resolved, facts.link)
        is MobilePaneFacts.Waste -> WastePaneExpanded(pane, facts.resolved)
        is MobilePaneFacts.Race -> RacePaneExpanded(pane, facts.resolved, nowMs)
        is MobilePaneFacts.Weekend ->
            WeekendPaneExpanded(pane, facts.resolved, onSetScheduledDate)
        // No card by choice, not for want of a lane — see the file header.
        is MobilePaneFacts.Vacation -> Unit
        is MobilePaneFacts.Scps -> ScpsPaneExpanded(facts.resolved, nowMs)
        is MobilePaneFacts.Kimi,
        is MobilePaneFacts.Github,
        is MobilePaneFacts.Uptime,
        is MobilePaneFacts.Reachability,
        is MobilePaneFacts.Poller ->
            error("a Status-surface question reached the Now expanded slot: ${pane.standingQuestion}")
    }
}

// -------------------------------------------------------------- homework

/** One open homework item's line — the title, and its deadline beside it
 * when it has one. A title and a meta line, deliberately, and the web's
 * pane is the same shape for the same reason: it was first built on
 * `ItemRow` and the visual gate caught that component ellipsising a title
 * down to `P.` in the 320px aside (`HomeworkPaneExpanded.tsx`'s header).
 * This surface has no `ItemRow` in the pane slot to be tempted by anyway.
 *
 * Read-only, exactly as the web's is: every affordance this could grow
 * already exists on the queue above and in the item pane, and #675's own
 * decision table is flat that the body is a read. */
@Composable
private fun HomeworkItemLine(item: MobileHomeworkItem, emphasis: Boolean) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(
            item.title,
            style = if (emphasis) {
                MaterialTheme.typography.titleMedium
            } else {
                MaterialTheme.typography.bodyMedium
            },
        )
        val deadline = item.deadline
        if (deadline != null) {
            Text(
                deadline,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun HomeworkFactsBody(facts: MobileHomeworkFacts) {
    val winner = facts.winner
    if (winner == null) {
        // An empty homework list is good news, reported as a fact — the
        // brand's own rule about empty states.
        Text(
            "Capture one with the @homework context and it shows up here.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        return
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        HomeworkItemLine(winner, emphasis = true)
        // The whole point of the pane: the preparation notes, in the
        // reader's own words, without going to find the item.
        val notes = winner.description
        if (notes != null) {
            Text(
                notes,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        if (facts.others.isNotEmpty()) {
            Text(
                if (facts.others.size == 1) "1 more open" else "${facts.others.size} more open",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            for (item in facts.others) {
                HomeworkItemLine(item, emphasis = false)
            }
        }
    }
}

/** The standing session link's label, and the web's own words for it
 * (`HomeworkPaneExpanded.tsx`) — pinned against them in `PaneAnswersTest`,
 * since the wording is per-client by ADR-0025 and nothing else would notice
 * the two drifting apart. */
internal const val HOMEWORK_LINK_LABEL = "Join the session"

@Composable
private fun HomeworkPaneExpanded(resolved: MobileHomeworkResolved, link: String?) {
    // The link is drawn in BOTH arms, the gap included: it is standing, and
    // it is not attached to the winning item at all. That is also why it
    // rides beside `resolved` on the seam rather than inside the facts —
    // the Gap arm carries none.
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        when (resolved) {
            is MobileHomeworkResolved.Gap -> Text(
                "Without this device's time zone there is no way to say which day a deadline falls on.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            is MobileHomeworkResolved.Facts -> HomeworkFactsBody(resolved.facts)
        }
        if (link != null) {
            val context = LocalContext.current
            TextButton(
                onClick = {
                    // `AlertDetailScreen.kt`'s own hand-off, verbatim. The
                    // core already refused anything that is not http(s), so
                    // this never hands the system an operator typo it would
                    // resolve to something else.
                    context.startActivity(
                        Intent(Intent.ACTION_VIEW, Uri.parse(link))
                            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                    )
                },
            ) {
                Text(HOMEWORK_LINK_LABEL)
                Spacer(Modifier.width(6.dp))
                // The web button's own trailing mark (`iconRight`), so both
                // clients warn that the tap leaves the app rather than only
                // one of them.
                Icon(
                    painterResource(R.drawable.ic_arrow_up_right),
                    contentDescription = null,
                    modifier = Modifier.size(15.dp),
                )
            }
        }
    }
}

// ------------------------------------------------------------------ scps

@Composable
private fun ScpsLaterEvent(event: MobileScpsEvent) {
    // Two lines, `HomeworkItemLine`'s precedent — `scpsCardTitle` already
    // spends " — " on the kind/topic relation, so reusing it here for the
    // title/day relation would read as the same separator meaning two
    // different things on one line.
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(
            scpsCardTitle(event),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(
            scpsDayLabel(event),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun ScpsPaneExpanded(resolved: MobileScpsResolved?, nowMs: Long) {
    // `resolved == null` mirrors `scps.rs`'s "never unbound" rule: this
    // pane has no setup step to be unbound from, so the only reason there
    // is nothing to draw is an unacquired calendar read — the same line
    // `ScpsPaneExpanded.tsx`'s own empty state uses.
    val facts = when (resolved) {
        null -> {
            Text(
                "Nothing to show until this device has read its calendars.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            return
        }
        // The zone bridge could not resolve a fact this pane needed —
        // unreachable in practice on a device that just resolved `nowMs`
        // into a `today`, but named rather than swallowed by an `else`.
        is MobileScpsResolved.Gap -> {
            Text(
                "Nothing to show until this device has read its calendars.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            return
        }
        is MobileScpsResolved.Facts -> resolved.facts
    }
    val next = facts.next
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (next == null) {
            Text("No SCPS event scheduled.", style = MaterialTheme.typography.headlineSmall)
        } else {
            Text(scpsCardTitle(next), style = MaterialTheme.typography.headlineSmall)
            Text(
                "${scpsDayLabel(next)} · ${scpsTimeLabel(next.startMs)}",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            val location = next.location
            if (location != null) {
                Text(
                    location,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            val notes = next.notes
            if (!notes.isNullOrBlank()) {
                Text(
                    notes,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        Text(
            scpsQuestLine(facts.quest, nowMs),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (facts.later.isNotEmpty()) {
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                for (event in facts.later) {
                    ScpsLaterEvent(event)
                }
            }
        }
        if (facts.stale) {
            Text(
                staleWords(facts.freshness),
                style = MaterialTheme.typography.labelSmall,
                color = warnColor(),
            )
        }
    }
}

// ----------------------------------------------------------------- waste

/** One kerbside bin, drawn — the web card's `Bin`: a lid bar over a
 * bordered, translucent body, in the bin's own real-world colours. */
@Composable
private fun BinFigure(stream: MobileWasteStream) {
    val colours = bin(stream)
    Column(
        modifier = Modifier.semantics { contentDescription = colours.label },
        verticalArrangement = Arrangement.spacedBy(3.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Column(
            modifier = Modifier
                .width(34.dp)
                .height(5.dp)
                .background(colours.edge, RoundedCornerShape(2.dp)),
        ) {}
        Column(
            modifier = Modifier
                .width(30.dp)
                .height(38.dp)
                .background(colours.fill, RoundedCornerShape(2.dp))
                .border(1.dp, colours.edge, RoundedCornerShape(2.dp)),
        ) {}
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun WastePaneExpanded(pane: MobileRankedPane, resolved: MobileWasteResolved) {
    // The shell already says "Not set up" and offers the Open Settings
    // door; this is the web setup card's body sentence under them.
    if (pane.answer.answerState == MobilePaneAnswerState.UNBOUND) {
        Text(
            "Set the council page your collection schedule is read from.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        return
    }
    val facts = when (resolved) {
        is MobileWasteResolved.Gap -> {
            Text(
                wasteGapReason(resolved.gap),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            return
        }
        is MobileWasteResolved.Facts -> resolved.facts
    }
    val weekday = WEEKDAYS[facts.weekdayIndex.toInt() % 7]
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterHorizontally),
        ) {
            for (stream in KERB_ORDER) {
                if (facts.streams.contains(stream)) {
                    BinFigure(stream)
                }
            }
        }
        Text(
            wasteExpandedHeadline(facts.daysAway, weekday, facts.holiday),
            style = MaterialTheme.typography.headlineSmall,
        )
        FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(
                "${weekday.take(3)} ${facts.collectedOn}",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (facts.holiday) {
                Text(
                    "holiday",
                    style = MaterialTheme.typography.labelSmall,
                    color = warnColor(),
                )
            }
            if (facts.stale) {
                Text(
                    staleWords(facts.freshness),
                    style = MaterialTheme.typography.labelSmall,
                    color = warnColor(),
                )
            }
        }
    }
}

// ------------------------------------------------------------------ race

@Composable
private fun RacePaneExpanded(pane: MobileRankedPane, resolved: MobileRaceResolved, nowMs: Long) {
    if (pane.answer.answerState == MobilePaneAnswerState.UNBOUND) {
        Text(
            "Name the racing series to follow, separated by commas.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        return
    }
    val facts: MobileRaceFacts = when (resolved) {
        is MobileRaceResolved.Gap -> {
            Text(
                raceGapReason(resolved.gap),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            return
        }
        is MobileRaceResolved.Facts -> resolved.facts
    }
    val event = facts.event
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                painterResource(if (facts.hasLiveAlert) R.drawable.ic_siren else R.drawable.ic_flag),
                contentDescription = null,
                modifier = Modifier.size(13.dp),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                seriesLabel(facts.series),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        if (event == null) {
            Text("No races scheduled", style = MaterialTheme.typography.headlineSmall)
        } else {
            val (value, unit) = countdown(event.startsAtMs - nowMs)
            Text(
                "${abbreviateEventName(event.name)} in $value $unit",
                style = MaterialTheme.typography.headlineSmall,
            )
        }
        val nextStart = facts.nextStart
        if (event != null && nextStart != null) {
            // The headline counts to race day; this line carries the thing
            // that actually happens first — Friday practice for most of a
            // race weekend, the race itself once the ladder is done.
            Text(
                "${nextStart.label} · ${raceDayLabel(nextStart.startsAtMs, nowMs)} " +
                    raceClock(nextStart.startsAtMs),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            // The circuit, and only the circuit: the headline already
            // names the event.
            Text(
                event.locality,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        if (facts.hasLiveAlert) {
            // The web shows the live alert's title beside this; only the
            // FACT crosses the mobile seam (`hasLiveAlert`), so the words
            // stop at the badge's.
            Text(
                "starting soon",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.error,
            )
        }
        if (facts.stale) {
            Text(
                staleWords(facts.freshness),
                style = MaterialTheme.typography.labelSmall,
                color = warnColor(),
            )
        }
    }
}

// --------------------------------------------------------------- weekend

@Composable
private fun WeekendPaneExpanded(
    pane: MobileRankedPane,
    resolved: MobileWeekendResolved,
    onSetScheduledDate: (itemId: String, date: String?) -> Unit,
) {
    if (pane.answer.answerState == MobilePaneAnswerState.UNBOUND) {
        // The shell already says "Not set up" and offers the Open Settings
        // door; this is the body sentence under them.
        Text(
            "Connect a calendar in Settings to see what the weekend already holds.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        return
    }
    val facts = when (resolved) {
        is MobileWeekendResolved.Gap -> {
            Text(
                weekendGapReason(resolved.gap),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            return
        }
        is MobileWeekendResolved.Facts -> resolved.facts
    }
    val zone = ZoneId.systemDefault()
    val dayKeys = facts.days.map { it.date }

    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (facts.days.all { it.entries.isEmpty() }) {
            Text(
                if (facts.window.underWay) "Nothing on so far." else "Nothing planned yet.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        for (day in facts.days) {
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(
                    shortDayLabel(day.date),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (day.entries.isEmpty()) {
                    Text(
                        "—",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                // Already in display order — `merge_window` sorted them,
                // and re-sorting here would be a second total order.
                for (entry in day.entries) {
                    WeekendEntryRow(entry, zone, dayKeys, onSetScheduledDate)
                }
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun WeekendEntryRow(
    entry: MobileWeekendEntry,
    zone: ZoneId,
    dayKeys: List<String>,
    onSetScheduledDate: (itemId: String, date: String?) -> Unit,
) {
    val planned = plannedDayOf(entry)
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .semantics { contentDescription = "${entry.title}, ${entry.kind.name.lowercase()}" },
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Text(entry.title, style = MaterialTheme.typography.bodyMedium)
        Text(
            listOfNotNull(
                weekendEntryTimeLabel(entry, zone),
                entry.deadlineOutsideWindow?.let { "due $it" },
            ).joinToString(" · "),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (offersPlanChips(entry)) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for (key in dayKeys) {
                    val on = planned == key
                    // Tapping the filled chip clears the do-date, exactly
                    // as the web's chip toggles — one control, two
                    // directions, so there is no separate "unplan".
                    TextButton(onClick = { onSetScheduledDate(entry.sourceId, if (on) null else key) }) {
                        Text(
                            shortDayLabel(key).uppercase(Locale.getDefault()),
                            style = MaterialTheme.typography.labelSmall,
                            color = if (on) {
                                MaterialTheme.colorScheme.primary
                            } else {
                                MaterialTheme.colorScheme.onSurfaceVariant
                            },
                        )
                    }
                }
            }
        }
    }
}
