package net.twinion.hummingbird.wear.questions

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.Text
import java.time.ZoneId
import net.twinion.hummingbird.ui.panes.KERB_ORDER
import net.twinion.hummingbird.ui.panes.WEEKDAYS
import net.twinion.hummingbird.ui.panes.abbreviateEventName
import net.twinion.hummingbird.ui.panes.bin
import net.twinion.hummingbird.ui.panes.countdown
import net.twinion.hummingbird.ui.panes.raceClock
import net.twinion.hummingbird.ui.panes.raceDayLabel
import net.twinion.hummingbird.ui.panes.raceGapReason
import net.twinion.hummingbird.ui.panes.scpsCardTitle
import net.twinion.hummingbird.ui.panes.scpsDayLabel
import net.twinion.hummingbird.ui.panes.scpsQuestLine
import net.twinion.hummingbird.ui.panes.scpsTimeLabel
import net.twinion.hummingbird.ui.panes.seriesLabel
import net.twinion.hummingbird.ui.panes.shortDayLabel
import net.twinion.hummingbird.ui.panes.staleWords
import net.twinion.hummingbird.ui.panes.wasteExpandedHeadline
import net.twinion.hummingbird.ui.panes.wasteGapReason
import net.twinion.hummingbird.ui.panes.weekendEntryTimeLabel
import net.twinion.hummingbird.ui.panes.weekendGapReason
import net.twinion.hummingbird.ui.theme.StatusWarnFgDark
import uniffi.hummingbird_ffi_mobile.MobileHomeworkFacts
import uniffi.hummingbird_ffi_mobile.MobileHomeworkItem
import uniffi.hummingbird_ffi_mobile.MobileHomeworkResolved
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswerState
import uniffi.hummingbird_ffi_mobile.MobilePaneFacts
import uniffi.hummingbird_ffi_mobile.MobilePaneFreshness
import uniffi.hummingbird_ffi_mobile.MobileRaceResolved
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileScpsResolved
import uniffi.hummingbird_ffi_mobile.MobileWasteResolved
import uniffi.hummingbird_ffi_mobile.MobileWeekendResolved

// The Now questions' expanded facts, on the wrist (ADR-0039): the phone's
// `NowPanesExpanded.kt` cards said in text alone, over the same `:brand`
// words — so what a card *says* cannot differ between the two devices, and
// only how much fits does. What is deliberately not here: the homework
// session link button (a URL is not a thing to open on a watch), the
// weekend plan chips (`setScheduledDate` is a decision made with a screen),
// the kerbside bin figures (a coloured dot each, from the same `bin()`
// colours), and the Settings door for an unbound question (its sentence
// stands alone). Vacation still has no card, for the phone's own reason.
//
// **Nothing here decides.** Bands, states and facts arrive on the pane; the
// clock is the one the rank was taken at. Exhaustive `when`s with no `else`
// throughout — `WearQuestionsStructuralTest` pins both rules.

@Composable
internal fun WearPaneExpanded(pane: MobileRankedPane, nowMs: Long) {
    when (val facts = pane.facts) {
        is MobilePaneFacts.Homework -> HomeworkBody(facts.resolved)
        is MobilePaneFacts.Scps -> ScpsBody(facts.resolved, nowMs)
        is MobilePaneFacts.Waste -> WasteBody(pane, facts.resolved)
        is MobilePaneFacts.Race -> RaceBody(pane, facts.resolved, nowMs)
        is MobilePaneFacts.Weekend -> WeekendBody(pane, facts.resolved)
        is MobilePaneFacts.Vacation -> Unit
        is MobilePaneFacts.Kimi,
        is MobilePaneFacts.Github,
        is MobilePaneFacts.Uptime,
        is MobilePaneFacts.Reachability,
        is MobilePaneFacts.Poller ->
            error("a Status-surface question reached the watch's Now list: ${pane.standingQuestion}")
    }
}

// -- the three registers every body uses --------------------------------

@Composable
private fun Body(text: String) {
    Text(text, style = MaterialTheme.typography.bodySmall)
}

@Composable
private fun Quiet(text: String) {
    Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

@Composable
private fun Meta(text: String, color: Color = MaterialTheme.colorScheme.onSurfaceVariant) {
    Text(text, style = MaterialTheme.typography.labelSmall, color = color)
}

@Composable
private fun StaleLine(freshness: MobilePaneFreshness) {
    Meta(staleWords(freshness), color = StatusWarnFgDark)
}

// -------------------------------------------------------------- homework

@Composable
private fun HomeworkLine(item: MobileHomeworkItem) {
    Column(verticalArrangement = Arrangement.spacedBy(1.dp)) {
        Body(item.title)
        item.deadline?.let { Meta(it) }
    }
}

@Composable
private fun HomeworkBody(resolved: MobileHomeworkResolved) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        when (resolved) {
            is MobileHomeworkResolved.Gap ->
                Quiet("Without this device's time zone there is no way to say which day a deadline falls on.")
            is MobileHomeworkResolved.Facts -> HomeworkFacts(resolved.facts)
        }
    }
}

@Composable
private fun HomeworkFacts(facts: MobileHomeworkFacts) {
    val winner = facts.winner
    if (winner == null) {
        Quiet("Capture one with the @homework context and it shows up here.")
        return
    }
    HomeworkLine(winner)
    winner.description?.let { Quiet(it) }
    if (facts.others.isNotEmpty()) {
        Meta(if (facts.others.size == 1) "1 more open" else "${facts.others.size} more open")
        for (item in facts.others) HomeworkLine(item)
    }
}

// ------------------------------------------------------------------ scps

@Composable
private fun ScpsBody(resolved: MobileScpsResolved?, nowMs: Long) {
    val facts = when (resolved) {
        null -> {
            Quiet("Nothing to show until this device has read its calendars.")
            return
        }
        is MobileScpsResolved.Gap -> {
            Quiet("Nothing to show until this device has read its calendars.")
            return
        }
        is MobileScpsResolved.Facts -> resolved.facts
    }
    val next = facts.next
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        if (next == null) {
            Body("No SCPS event scheduled.")
        } else {
            Body(scpsCardTitle(next))
            Quiet("${scpsDayLabel(next)} · ${scpsTimeLabel(next.startMs)}")
            next.location?.let { Quiet(it) }
            val notes = next.notes
            if (!notes.isNullOrBlank()) Quiet(notes)
        }
        Meta(scpsQuestLine(facts.quest, nowMs))
        for (event in facts.later) {
            Quiet(scpsCardTitle(event))
            Meta(scpsDayLabel(event))
        }
        if (facts.stale) StaleLine(facts.freshness)
    }
}

// ----------------------------------------------------------------- waste

@Composable
private fun WasteBody(pane: MobileRankedPane, resolved: MobileWasteResolved) {
    if (pane.answer.answerState == MobilePaneAnswerState.UNBOUND) {
        Quiet("Set the council page your collection schedule is read from.")
        return
    }
    val facts = when (resolved) {
        is MobileWasteResolved.Gap -> {
            Quiet(wasteGapReason(resolved.gap))
            return
        }
        is MobileWasteResolved.Facts -> resolved.facts
    }
    val weekday = WEEKDAYS[facts.weekdayIndex.toInt() % 7]
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        // One dot per bin out that day, in kerb order, in the bin's own
        // colours — the phone's bin figures reduced to what a 1.2-inch
        // display can afford.
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            for (stream in KERB_ORDER) {
                if (facts.streams.contains(stream)) {
                    val colours = bin(stream)
                    Box(Modifier.size(10.dp).background(colours.fill, CircleShape))
                }
            }
        }
        Body(wasteExpandedHeadline(facts.daysAway, weekday, facts.holiday))
        Meta("${weekday.take(3)} ${facts.collectedOn}")
        if (facts.holiday) Meta("holiday", color = StatusWarnFgDark)
        if (facts.stale) StaleLine(facts.freshness)
    }
}

// ------------------------------------------------------------------ race

@Composable
private fun RaceBody(pane: MobileRankedPane, resolved: MobileRaceResolved, nowMs: Long) {
    if (pane.answer.answerState == MobilePaneAnswerState.UNBOUND) {
        Quiet("Name the racing series to follow, separated by commas.")
        return
    }
    val facts = when (resolved) {
        is MobileRaceResolved.Gap -> {
            Quiet(raceGapReason(resolved.gap))
            return
        }
        is MobileRaceResolved.Facts -> resolved.facts
    }
    val event = facts.event
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Meta(seriesLabel(facts.series))
        if (event == null) {
            Body("No races scheduled")
        } else {
            val (value, unit) = countdown(event.startsAtMs - nowMs)
            Body("${abbreviateEventName(event.name)} in $value $unit")
        }
        val nextStart = facts.nextStart
        if (event != null && nextStart != null) {
            Quiet("${nextStart.label} · ${raceDayLabel(nextStart.startsAtMs, nowMs)} ${raceClock(nextStart.startsAtMs)}")
            Quiet(event.locality)
        }
        if (facts.hasLiveAlert) Meta("starting soon", color = MaterialTheme.colorScheme.error)
        if (facts.stale) StaleLine(facts.freshness)
    }
}

// --------------------------------------------------------------- weekend

@Composable
private fun WeekendBody(pane: MobileRankedPane, resolved: MobileWeekendResolved) {
    if (pane.answer.answerState == MobilePaneAnswerState.UNBOUND) {
        Quiet("Connect a calendar in Settings to see what the weekend already holds.")
        return
    }
    val facts = when (resolved) {
        is MobileWeekendResolved.Gap -> {
            Quiet(weekendGapReason(resolved.gap))
            return
        }
        is MobileWeekendResolved.Facts -> resolved.facts
    }
    val zone = ZoneId.systemDefault()
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        if (facts.days.all { it.entries.isEmpty() }) {
            Quiet(if (facts.window.underWay) "Nothing on so far." else "Nothing planned yet.")
        }
        for (day in facts.days) {
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Meta(shortDayLabel(day.date))
                if (day.entries.isEmpty()) Quiet("—")
                // Already in display order — `merge_window` sorted them.
                for (entry in day.entries) {
                    Body(entry.title)
                    Quiet(
                        listOfNotNull(
                            weekendEntryTimeLabel(entry, zone),
                            entry.deadlineOutsideWindow?.let { "due $it" },
                        ).joinToString(" · "),
                    )
                }
            }
        }
    }
}
