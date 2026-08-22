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
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale
import net.twinion.hummingbird.R
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswerState
import uniffi.hummingbird_ffi_mobile.MobileHomeworkFacts
import uniffi.hummingbird_ffi_mobile.MobileHomeworkItem
import uniffi.hummingbird_ffi_mobile.MobileHomeworkResolved
import uniffi.hummingbird_ffi_mobile.MobilePaneFacts
import uniffi.hummingbird_ffi_mobile.MobileRaceFacts
import uniffi.hummingbird_ffi_mobile.MobileRaceGap
import uniffi.hummingbird_ffi_mobile.MobileRaceResolved
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileWasteGap
import uniffi.hummingbird_ffi_mobile.MobileWasteResolved
import uniffi.hummingbird_ffi_mobile.MobileWasteStream
import uniffi.hummingbird_ffi_mobile.MobileWeekendEntry
import uniffi.hummingbird_ffi_mobile.MobileWeekendEntryAnchor
import uniffi.hummingbird_ffi_mobile.MobileWeekendEntryKind
import uniffi.hummingbird_ffi_mobile.MobileWeekendGap
import uniffi.hummingbird_ffi_mobile.MobileWeekendResolved

// The Now surface's expanded renderings (the pane-content slice, second
// half) — homework (#675), waste and race, each web `*PaneExpanded.tsx`
// ported: the bins do
// the talking on waste (real kerb colours, three words, a date), and the
// race card is the prototype's series tile under real rules — plus, since
// #564/#621, the weekend card. Its entries are `weekend.rs`'s own merged
// `days` (sunk at #564 precisely so this card and `WeekendPaneExpanded.tsx`
// cannot disagree about the due-beats-scheduled dedupe), and its plan chips
// are `MobileTaskHost.setScheduledDate`'s first caller.
//
// **Vacation still has no card**, and that is a scope line rather than a
// missing lane now: the trips themselves cross fine, but `MobileTrip`
// carries no event title, so a card here would name every trip by its
// location or "a trip" — see `PaneAnswers.kt`'s `vacationTripHeadline` for
// the same recorded divergence. The shell's collapsed headline is the
// honest whole story until the seam grows the name the web derives.
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
        // Slice 2 (#693) sank the decision core-side only; the Android card
        // is slice 3's own scope (#694) — the same "no card yet" shape
        // `Vacation`'s own arm above already uses, not a regression.
        is MobilePaneFacts.Scps -> Unit
        is MobilePaneFacts.Kimi,
        is MobilePaneFacts.Github,
        is MobilePaneFacts.Uptime,
        is MobilePaneFacts.Reachability ->
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

// ----------------------------------------------------------------- waste

/** `wasteHeadline` in `waste.ts` (the EXPANDED card's sentence — wordier
 * than the collapsed `wasteCollapsedHeadline`), ported verbatim, capital
 * letters included. */
internal fun wasteExpandedHeadline(daysAway: Long, weekday: String, holiday: Boolean): String {
    if (daysAway == 0L) return "Trash Today"
    if (holiday) return "Trash $weekday"
    if (daysAway == 1L) return "Trash Tonight"
    return "Trash $weekday"
}

/** `gapReason` in `waste.ts`, ported per kind. */
internal fun wasteGapReason(gap: MobileWasteGap): String = when (gap) {
    MobileWasteGap.NotFetched -> "No collection schedule has been fetched yet."
    is MobileWasteGap.Malformed -> "The collection payload couldn't be read: ${gap.reason}"
    is MobileWasteGap.UnknownSchema ->
        "This device doesn't know how to read ${gap.schema} yet. Update the app."
    MobileWasteGap.NotJson -> "The collection payload isn't JSON."
    MobileWasteGap.NotAnObject -> "The collection payload isn't an object."
    MobileWasteGap.NoZone -> "The collection payload names no time zone."
    MobileWasteGap.BadDates -> "The collection payload's dates aren't whole days."
    MobileWasteGap.UnknownStream -> "The collection payload lists an unknown kind of bin."
    is MobileWasteGap.UnresolvableZone ->
        "The collection payload names an unknown time zone (${gap.zone})."
    is MobileWasteGap.PastCollection ->
        "The collection schedule is out of date: it still names " +
            "${WEEKDAYS[gap.weekdayIndex.toInt() % 7]} ${gap.collectedOn}, which has passed."
}

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

/** `gapReason` in `race.ts`, ported per kind. */
internal fun raceGapReason(gap: MobileRaceGap): String = when (gap) {
    MobileRaceGap.NotFetched -> "No schedule has been fetched for this series yet."
    is MobileRaceGap.Malformed -> "The schedule payload couldn't be read: ${gap.reason}"
    is MobileRaceGap.UnknownSchema ->
        "This device doesn't know how to read ${gap.schema} yet. Update the app."
    MobileRaceGap.NotJson -> "The schedule payload isn't JSON."
    MobileRaceGap.NotAnObject -> "The schedule payload isn't an object."
    MobileRaceGap.NoSeason -> "The schedule payload carries no season."
    MobileRaceGap.BadEvent -> "The schedule payload lists an event this app can't read."
}

/** `dayLabel` in `race.ts`, ported onto `java.time` — Today, Tomorrow, the
 * weekday inside a week, else "Aug 24"; civil days in the DEVICE's zone,
 * the same device-local reading ADR-0015 gives the whole pane. */
internal fun raceDayLabel(atMs: Long, nowMs: Long, zone: ZoneId = ZoneId.systemDefault()): String {
    val at = Instant.ofEpochMilli(atMs).atZone(zone).toLocalDate()
    val today = Instant.ofEpochMilli(nowMs).atZone(zone).toLocalDate()
    val days = at.toEpochDay() - today.toEpochDay()
    if (days == 0L) return "Today"
    if (days == 1L) return "Tomorrow"
    if (days in 2..6) {
        return at.format(DateTimeFormatter.ofPattern("EEEE", Locale.US))
    }
    return at.format(DateTimeFormatter.ofPattern("MMM d", Locale.US))
}

/** `clock` in `race.ts` — "4:00 PM", the wall clock in the device's own
 * zone, and no zone suffix (ADR-0015 is device-local). */
internal fun raceClock(atMs: Long, zone: ZoneId = ZoneId.systemDefault()): String =
    Instant.ofEpochMilli(atMs)
        .atZone(zone)
        .format(DateTimeFormatter.ofPattern("h:mm a", Locale.US))

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

/** Why an answered-looking weekend pane has nothing to draw, per gap kind
 * — the expanded card's own sentence, wordier than the collapsed
 * headline's "Checking calendar", exactly as `wasteGapReason` is wordier
 * than its headline. Exhaustive with no `else`: a fourth gap in the core's
 * enum is a compile error here, not a card that renders as nothing. */
internal fun weekendGapReason(gap: MobileWeekendGap): String = when (gap) {
    MobileWeekendGap.NOT_CONNECTED ->
        "Connect a calendar in Settings to see what the weekend already holds."
    MobileWeekendGap.UNACQUIRED -> "This device hasn't read the calendar yet."
    MobileWeekendGap.UNRESOLVABLE_ZONE ->
        "This device's own time zone couldn't be resolved, so the weekend can't be placed."
}

/** The short weekday a plan chip is labelled with — "Fri"/"Sat"/"Sun",
 * from the day's own civil date. `weekend.ts`'s `shortDayLabel`, ported.
 *
 * Parsed as a civil date and formatted in the device's own zone, which is
 * the only zone this pane has: the day key came from `weekend.rs`'s window,
 * resolved through `DEVICE_ZONE` in the first place. */
internal fun shortDayLabel(dayKey: String): String = runCatching {
    java.time.LocalDate.parse(dayKey).format(DateTimeFormatter.ofPattern("EEE", Locale.getDefault()))
}.getOrDefault(dayKey)

/** One entry's time words — `weekend.ts`'s `timeLabel`, ported.
 *
 * Reads the entry's own `anchor` rather than inferring one from the
 * timestamp: whether something covers a day or sits at an instant is the
 * core's answer (`weekend.rs`'s `EntryAnchor`), and a Kotlin
 * `atMs % DAY_MS == 0` test would be a second, worse copy of it. */
internal fun weekendEntryTimeLabel(entry: MobileWeekendEntry, zone: ZoneId): String =
    when (entry.kind) {
        MobileWeekendEntryKind.SCHEDULED -> "anytime"
        MobileWeekendEntryKind.DUE ->
            when (entry.anchor) {
                MobileWeekendEntryAnchor.DAY -> "by end of day"
                MobileWeekendEntryAnchor.TIME -> "by ${clockOf(entry.atMs, zone)}"
            }
        MobileWeekendEntryKind.EVENT ->
            when (entry.anchor) {
                MobileWeekendEntryAnchor.DAY -> "all day"
                MobileWeekendEntryAnchor.TIME -> clockOf(entry.atMs, zone)
            }
    }

private fun clockOf(atMs: Long, zone: ZoneId): String =
    DateTimeFormatter.ofPattern("h:mm a", Locale.getDefault())
        .format(Instant.ofEpochMilli(atMs).atZone(zone))

/** Which day this entry is currently planned for, or `null` if none —
 * `PlanChips`' own `planned`, ported. A `scheduled` entry is planned for
 * its own day; a `due` entry that ALSO has a do-date inside the window
 * carries it as `alsoScheduledOn`, because the merge deduped the second
 * entry away (`weekend.rs`'s `merge_window`). Nothing else is planned. */
internal fun plannedDayOf(entry: MobileWeekendEntry): String? = when (entry.kind) {
    MobileWeekendEntryKind.SCHEDULED -> entry.dayKey
    MobileWeekendEntryKind.DUE -> entry.alsoScheduledOn
    MobileWeekendEntryKind.EVENT -> null
}

/** Whether a plan chip is offered at all: only for something with an item
 * behind it. An event is the calendar's, and this app writes no calendar
 * (ADR-0002 rule 1 — a context mirror cannot mint or modify anything). */
internal fun offersPlanChips(entry: MobileWeekendEntry): Boolean =
    entry.kind != MobileWeekendEntryKind.EVENT

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
