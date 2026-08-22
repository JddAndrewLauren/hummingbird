package net.twinion.hummingbird.ui.panes

import androidx.compose.ui.graphics.Color
import java.util.Locale
import kotlin.math.abs
import kotlin.math.roundToLong
import net.twinion.hummingbird.R
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswerState
import uniffi.hummingbird_ffi_mobile.MobilePaneBand
import uniffi.hummingbird_ffi_mobile.MobilePaneFacts
import uniffi.hummingbird_ffi_mobile.MobileHomeworkFacts
import uniffi.hummingbird_ffi_mobile.MobileHomeworkResolved
import uniffi.hummingbird_ffi_mobile.MobilePaneFreshness
import uniffi.hummingbird_ffi_mobile.MobileProbeBody
import uniffi.hummingbird_ffi_mobile.MobileProbeExpected
import uniffi.hummingbird_ffi_mobile.MobileProbeResolved
import uniffi.hummingbird_ffi_mobile.MobileRaceResolved
import uniffi.hummingbird_ffi_mobile.MobileRaceSetup
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileKimiResolved
import uniffi.hummingbird_ffi_mobile.MobileScpsEvent
import uniffi.hummingbird_ffi_mobile.MobileScpsKind
import uniffi.hummingbird_ffi_mobile.MobileScpsResolved
import uniffi.hummingbird_ffi_mobile.MobileTrip
import uniffi.hummingbird_ffi_mobile.MobileTripPhase
import uniffi.hummingbird_ffi_mobile.MobileVacationResolved
import uniffi.hummingbird_ffi_mobile.MobileWasteResolved
import uniffi.hummingbird_ffi_mobile.MobileWasteSetup
import uniffi.hummingbird_ffi_mobile.MobileWasteStream
import uniffi.hummingbird_ffi_mobile.MobileWeekendCounts
import uniffi.hummingbird_ffi_mobile.MobileWeekendResolved
import uniffi.hummingbird_ffi_mobile.MobileWorkflowResolved

// Every pane's collapsed headline and glyphs (#537's shell, brought to the
// web's own two-form contract by the pane-parity slice): the SENTENCE and
// the MARKS for a collapsed row, composed per question from the decided
// facts `MobileRankedPane.facts` carries since the pane-facts seam slice —
// exactly the cut ADR-0025 draws through the web's `PaneAnswer`
// (`answerState`/`band`/`withinBand` decided in Rust; `collapsedHeadline`/
// `icon` per-client). Each section ports its web twin's wording verbatim
// (`client/web/src/screens/<q>-pane/<q>.ts`), so the two clients read the
// same answer in the same words.
//
// **Nothing here re-derives a decision.** The band and answer state are
// read off the pane; no function in this file returns a
// [MobilePaneBand], and the day/hour arithmetic below is words-about-an-
// instant (countdowns, ages), never a banding rule. Where the web's
// stale-escalation wording recomputes a raw band to tell "genuinely
// imminent" from "escalated because stale" (github/uptime), this file
// deliberately prefers the staleness caveat whenever `stale` is set on an
// imminent pane — both sentences are true when both apply, and recomputing
// the raw band here would be the second banding implementation the
// structural gate exists to refuse.
//
// Exhaustive `when`s with no `else` arm throughout — the house drift gate:
// a ninth question, a new gap kind or a sixth band added core-side must
// fail this build, never render as a silently-wrong sentence.

/** The collapsed row's whole sentence for one ranked pane. */
internal fun paneHeadline(pane: MobileRankedPane, nowMs: Long): String = when (val facts = pane.facts) {
    is MobilePaneFacts.Homework -> homeworkHeadline(facts.resolved)
    is MobilePaneFacts.Scps -> scpsHeadline(pane, facts.resolved)
    is MobilePaneFacts.Waste -> wasteHeadline(pane, facts.setup, facts.resolved)
    is MobilePaneFacts.Weekend -> weekendHeadline(pane, facts.resolved)
    is MobilePaneFacts.Vacation -> vacationHeadline(pane, facts.resolved)
    is MobilePaneFacts.Race -> raceHeadline(pane, facts.setup, facts.resolved, nowMs)
    is MobilePaneFacts.Kimi -> kimiHeadline(pane, facts.resolved)
    is MobilePaneFacts.Github -> githubHeadline(pane, facts.resolved, nowMs)
    is MobilePaneFacts.Uptime -> uptimeHeadline(pane, facts.resolved)
    is MobilePaneFacts.Reachability -> reachabilityHeadline(facts.facts?.let {
        ReachabilityWords(it.ageMs, it.stale, it.latestAttemptLanded)
    })
}

/** The collapsed row's marks for one ranked pane — unbounded here; the
 * shell applies [MAX_GLYPHS]. */
internal fun paneGlyphs(pane: MobileRankedPane, nowMs: Long): List<PaneGlyph> = when (val facts = pane.facts) {
    is MobilePaneFacts.Homework -> homeworkGlyphs(facts.resolved)
    is MobilePaneFacts.Scps -> emptyList()
    is MobilePaneFacts.Waste -> wasteGlyphs(pane, facts.setup, facts.resolved)
    is MobilePaneFacts.Weekend -> weekendGlyphs(pane, facts.resolved)
    is MobilePaneFacts.Vacation -> emptyList()
    is MobilePaneFacts.Race -> raceGlyphs(pane, facts.setup, facts.resolved)
    is MobilePaneFacts.Kimi -> kimiGlyphs(pane)
    is MobilePaneFacts.Github -> githubGlyphs(pane, facts.resolved, nowMs)
    is MobilePaneFacts.Uptime -> uptimeGlyphs(pane, facts.resolved)
    is MobilePaneFacts.Reachability -> reachabilityGlyphs(facts.facts?.stale)
}

// ---------------------------------------------------------------- shared

private fun answered(pane: MobileRankedPane): Boolean =
    pane.answer.answerState == MobilePaneAnswerState.ANSWERED

private val NOT_SET_UP = PaneGlyph.Icon(R.drawable.ic_help_circle, "not set up")
private val NO_ANSWER = PaneGlyph.Icon(R.drawable.ic_cloud_fog, "no answer yet")

/** The two setup-kind marks the waste and race panes share — `waste.ts` and
 * `race.ts` draw the same pair for the same two arms. */
private val CHECKING_SETUP = PaneGlyph.Icon(R.drawable.ic_cloud_fog, "checking setup")
private val SETUP_NEEDS_A_LOOK = PaneGlyph.Icon(R.drawable.ic_help_circle, "setup needs a look")

/** `ageWords` in `github.ts`/`uptime.ts`, ported — internal since the
 * pane-content slice: the expanded cards speak the same ages. */
internal fun ageWords(ageMs: Long): String {
    val hours = ageMs / 3_600_000
    if (hours < 1) return "under an hour ago"
    if (hours < 48) return "${hours}h ago"
    return "${hours / 24}d ago"
}

private fun heardAgo(freshness: MobilePaneFreshness): String = when (freshness) {
    is MobilePaneFreshness.Age -> ageWords(freshness.ageMs)
    MobilePaneFreshness.Unknown -> "an unknown time ago"
}

// ----------------------------------------------------------------- waste

/** The bins' own colours, as they are on the kerb — `BIN` in `waste.ts`,
 * ported (its doc records why these are literal hex, not brand tokens:
 * they encode OBJECT identity, the one thing the reader matches against
 * the real world before walking outside). */
internal data class Bin(val fill: Color, val edge: Color, val label: String)

internal fun bin(stream: MobileWasteStream): Bin = when (stream) {
    MobileWasteStream.TRASH -> Bin(Color(0x739AA3AB), Color(0xFF79838B), "trash")
    MobileWasteStream.RECYCLING -> Bin(Color(0x737FC4E8), Color(0xFF3F93C4), "recycling")
    MobileWasteStream.YARD -> Bin(Color(0x736AA84F), Color(0xFF4D8A3A), "yard")
}

/** Kerb order, never the order the payload happened to list —
 * `STREAM_ORDER`/`orderedStreams` in `waste.ts`, ported (the core carries
 * the list in payload order on purpose; kerb order is the client's). */
internal val KERB_ORDER = listOf(
    MobileWasteStream.TRASH,
    MobileWasteStream.RECYCLING,
    MobileWasteStream.YARD,
)

/** `WEEKDAYS` in `waste.ts` — the word is per-client, the day is the
 * core's `weekdayIndex` (0 = Sunday). */
internal val WEEKDAYS = listOf(
    "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday",
)

private fun wasteHeadline(
    pane: MobileRankedPane,
    setup: MobileWasteSetup,
    resolved: MobileWasteResolved,
): String {
    when (pane.answer.answerState) {
        MobilePaneAnswerState.UNBOUND -> return "Not set up"
        // Which gap it is decides the words — `waste.ts`'s own branch,
        // ported: the bindings table has not been read on this device yet,
        // it holds something this pane cannot use, or it is set and there is
        // simply no answer. The three share one answer state, so the setup
        // kind (`MobileWasteSetup`, carried on the facts for exactly this)
        // is what separates them; saying "No answer yet" for all three
        // hides a repair the reader could make in Settings.
        MobilePaneAnswerState.BOUND_BUT_UNACQUIRED -> return when (setup) {
            MobileWasteSetup.UNREAD -> "Checking setup"
            MobileWasteSetup.UNUSABLE -> "Setup needs a look"
            MobileWasteSetup.BOUND, MobileWasteSetup.UNSET -> "No answer yet"
        }
        MobilePaneAnswerState.ANSWERED -> Unit
    }
    return when (resolved) {
        // Unreachable: the core answers `answered` only when it produced
        // facts — the same note `waste.ts` carries on this branch.
        is MobileWasteResolved.Gap -> "No answer yet"
        is MobileWasteResolved.Facts -> wasteCollapsedHeadline(
            daysAway = resolved.facts.daysAway,
            weekday = WEEKDAYS[resolved.facts.weekdayIndex.toInt() % 7],
            holiday = resolved.facts.holiday,
        )
    }
}

/** `wasteCollapsedHeadline` in `waste.ts`, ported verbatim — including the
 * `daysAway == 0` equality its doc defends (never `<= 0`). */
internal fun wasteCollapsedHeadline(daysAway: Long, weekday: String, holiday: Boolean): String {
    if (daysAway == 0L) return "Trash today"
    if (daysAway == 1L && !holiday) return "Trash tonight"
    return "$weekday · ${daysAway}d"
}

private fun wasteGlyphs(
    pane: MobileRankedPane,
    setup: MobileWasteSetup,
    resolved: MobileWasteResolved,
): List<PaneGlyph> {
    when (pane.answer.answerState) {
        MobilePaneAnswerState.UNBOUND -> return listOf(NOT_SET_UP)
        MobilePaneAnswerState.BOUND_BUT_UNACQUIRED -> return listOf(
            when (setup) {
                MobileWasteSetup.UNREAD -> CHECKING_SETUP
                MobileWasteSetup.UNUSABLE -> SETUP_NEEDS_A_LOOK
                MobileWasteSetup.BOUND, MobileWasteSetup.UNSET -> NO_ANSWER
            },
        )
        MobilePaneAnswerState.ANSWERED -> Unit
    }
    return when (resolved) {
        is MobileWasteResolved.Gap -> listOf(NO_ANSWER)
        is MobileWasteResolved.Facts -> KERB_ORDER
            .filter { resolved.facts.streams.contains(it) }
            .map { stream ->
                val colours = bin(stream)
                PaneGlyph.Dot(colours.fill, colours.edge, colours.label)
            }
    }
}

// -------------------------------------------------------------- homework

private fun homeworkHeadline(resolved: MobileHomeworkResolved): String = when (resolved) {
    // The device could not say what day it is here. Never "Not set up":
    // nobody binds this question (`homework.rs`'s own doc), so there is
    // nothing to route anyone to.
    is MobileHomeworkResolved.Gap -> "Can't read this device's time zone"
    is MobileHomeworkResolved.Facts -> homeworkCollapsedHeadline(resolved.facts)
}

/** `homeworkHeadline` in `homework.ts`, ported word for word — the forms
 * are #675's own decision table, and the two clients must read the same
 * answer in the same words. */
internal fun homeworkCollapsedHeadline(facts: MobileHomeworkFacts): String {
    if (facts.winner == null) return "No open homework"
    // No deadline on it: saying "Homework" and stopping is the honest
    // version — a fabricated "someday" would invent a date nobody set.
    val away = facts.daysAway ?: return "Homework"
    // An if-chain rather than a `when { … else -> }`: `PaneShellStructuralTest`
    // bans a wildcard arm anywhere in this file, and the ban is worth more
    // than the two lines this would save.
    if (away < 0L) return "Homework ${homeworkDays(-away)} overdue"
    if (away == 0L) return "Homework due today"
    if (away == 1L) return "Homework due tomorrow"
    return "Homework due in ${homeworkDays(away)}"
}

private fun homeworkDays(count: Long): String = if (count == 1L) "1 day" else "$count days"

private fun homeworkGlyphs(resolved: MobileHomeworkResolved): List<PaneGlyph> = when (resolved) {
    is MobileHomeworkResolved.Gap ->
        listOf(PaneGlyph.Icon(R.drawable.ic_cloud_fog, "time zone unreadable"))
    is MobileHomeworkResolved.Facts -> {
        val facts = resolved.facts
        if (facts.winner == null) {
            listOf(PaneGlyph.Icon(R.drawable.ic_circle_check, "nothing open"))
        } else {
            buildList {
                val days = facts.daysAway
                add(
                    if (days != null && days <= 0L) {
                        PaneGlyph.Icon(R.drawable.ic_flag, "due")
                    } else {
                        PaneGlyph.Icon(R.drawable.ic_scroll_text, "homework")
                    },
                )
                if (facts.others.isNotEmpty()) {
                    add(
                        PaneGlyph.Icon(
                            R.drawable.ic_list_checks,
                            "${facts.others.size} more open",
                        ),
                    )
                }
            }
        }
    }
}

// --------------------------------------------------------------- weekend

private fun weekendHeadline(pane: MobileRankedPane, resolved: MobileWeekendResolved): String {
    when (pane.answer.answerState) {
        MobilePaneAnswerState.UNBOUND -> return "Not set up"
        MobilePaneAnswerState.BOUND_BUT_UNACQUIRED -> return "Checking calendar"
        MobilePaneAnswerState.ANSWERED -> Unit
    }
    return when (resolved) {
        is MobileWeekendResolved.Gap -> "Checking calendar"
        is MobileWeekendResolved.Facts -> weekendCollapsedHeadline(
            resolved.facts.counts,
            resolved.facts.window.underWay,
        )
    }
}

/** `weekendCollapsedHeadline` in `weekend.ts`, ported — counts only, never
 * a per-entry list (the core module's own call). */
internal fun weekendCollapsedHeadline(counts: MobileWeekendCounts, underWay: Boolean): String {
    val total = counts.events + counts.due + counts.scheduled
    if (total == 0L) return if (underWay) "Clear so far" else "Nothing planned"
    val parts = mutableListOf<String>()
    if (counts.due > 0) parts.add("${counts.due} due")
    if (counts.events > 0) parts.add("${counts.events} on the calendar")
    if (counts.scheduled > 0) parts.add("${counts.scheduled} planned")
    return parts.joinToString(" · ")
}

private fun weekendGlyphs(pane: MobileRankedPane, resolved: MobileWeekendResolved): List<PaneGlyph> {
    when (pane.answer.answerState) {
        MobilePaneAnswerState.UNBOUND -> return listOf(NOT_SET_UP)
        MobilePaneAnswerState.BOUND_BUT_UNACQUIRED ->
            return listOf(PaneGlyph.Icon(R.drawable.ic_cloud_fog, "checking calendar"))
        MobilePaneAnswerState.ANSWERED -> Unit
    }
    return when (resolved) {
        is MobileWeekendResolved.Gap ->
            listOf(PaneGlyph.Icon(R.drawable.ic_cloud_fog, "checking calendar"))
        is MobileWeekendResolved.Facts -> {
            val counts = resolved.facts.counts
            val glyphs = mutableListOf<PaneGlyph>()
            if (counts.due > 0) {
                glyphs.add(PaneGlyph.Icon(R.drawable.ic_flag, "${counts.due} due this weekend"))
            }
            if (counts.events > 0) {
                glyphs.add(
                    PaneGlyph.Icon(R.drawable.ic_calendar_clock, "${counts.events} on the calendar"),
                )
            }
            if (counts.scheduled > 0) {
                glyphs.add(PaneGlyph.Icon(R.drawable.ic_calendar, "${counts.scheduled} planned"))
            }
            glyphs
        }
    }
}

// ------------------------------------------------------------------ scps

/** `scps.ts`'s `scpsHeadline`, ported at collapsed-row depth only (#693):
 * the day and the topic, never the exact clock time or the Photo Quest —
 * that finer body is #694's own scope (Android's expanded card), the same
 * "no card yet" line `NowPanesExpanded.kt`'s `Vacation` arm already draws.
 * Never `unbound` here — `scps.rs`'s own rule, ADR-0015's #693 amendment. */
private fun scpsHeadline(pane: MobileRankedPane, resolved: MobileScpsResolved?): String {
    when (pane.answer.answerState) {
        MobilePaneAnswerState.UNBOUND, MobilePaneAnswerState.BOUND_BUT_UNACQUIRED ->
            return "Waiting for the first calendar sync"
        MobilePaneAnswerState.ANSWERED -> Unit
    }
    return when (resolved) {
        null -> "Waiting for the first calendar sync"
        is MobileScpsResolved.Gap -> "Waiting for the first calendar sync"
        is MobileScpsResolved.Facts -> scpsEventHeadline(resolved.facts.next)
    }
}

private fun scpsEventHeadline(next: MobileScpsEvent?): String {
    if (next == null) return "No SCPS event scheduled"
    if (next.kind == MobileScpsKind.HAPPY_HOUR && next.inProgress) {
        return "SCPS Happy Hour in progress"
    }
    val kindWord = when (next.kind) {
        MobileScpsKind.MEETING -> "Meeting"
        MobileScpsKind.ACTIVITY -> "Activity"
        MobileScpsKind.HAPPY_HOUR -> "Happy Hour"
        MobileScpsKind.EVENT -> "event"
    }
    // An if-chain rather than a `when { … else -> }` — `PaneShellStructuralTest`
    // bans a wildcard arm anywhere in this file (`homeworkCollapsedHeadline`'s
    // own precedent above).
    val day = if (next.inProgress || next.daysUntil <= 0) {
        "today"
    } else if (next.daysUntil == 1L) {
        "tomorrow"
    } else {
        "in ${next.daysUntil} days"
    }
    val topic = if (!next.topic.isNullOrBlank()) " — ${next.topic}" else ""
    return "SCPS $kindWord $day$topic"
}

// -------------------------------------------------------------- vacation

private fun vacationHeadline(pane: MobileRankedPane, resolved: MobileVacationResolved?): String {
    when (pane.answer.answerState) {
        MobilePaneAnswerState.UNBOUND -> return "Not set up"
        MobilePaneAnswerState.BOUND_BUT_UNACQUIRED -> return "Waiting for the first calendar sync"
        MobilePaneAnswerState.ANSWERED -> Unit
    }
    return when (resolved) {
        null -> "Waiting for the first calendar sync"
        is MobileVacationResolved.Gap -> "Waiting for the first calendar sync"
        is MobileVacationResolved.Facts -> vacationTripHeadline(resolved.facts.next)
    }
}

/** `vacationHeadline` in `vacation.ts`, ported — `HORIZON_LABEL` is that
 * file's own "2 years", the display spelling of `vacation.rs`'s 730-day
 * `HORIZON_AHEAD_DAYS`.
 *
 * **The name here is NOT the web's trip name, and the divergence is
 * deliberate.** The web enriches each core trip with a `name` derived from
 * the calendar EVENT TITLE (`vacation.ts`'s `withName`/`tripName`: "Trip:
 * India" reads as "India", falling back to the event id). `MobileTrip`
 * carries no title — the seam's `location` is the event's separate location
 * field (`decisions/panes/vacation.rs`) — so this reads the location and
 * says "a trip" when there is none. Reachable since #621 landed the calendar
 * lane, so it is wrong-looking output today: closing it means the seam
 * growing the name the web derives from the event title (#621 records this
 * as its own scope line). */
internal fun vacationTripHeadline(next: MobileTrip?): String {
    if (next == null) return "Nothing booked in the next 2 years"
    val name = next.location ?: "a trip"
    return when (next.phase) {
        MobileTripPhase.UPCOMING ->
            if (next.daysUntil == 1L) "$name tomorrow" else "$name in ${next.daysUntil} days"
        MobileTripPhase.DEPARTS_TODAY -> "$name today"
        MobileTripPhase.UNDER_WAY -> "In $name · day ${next.dayOfTrip} of ${next.lengthDays}"
        MobileTripPhase.RETURNS_TODAY -> "Home today from $name"
        MobileTripPhase.PAST -> "$name is over"
    }
}

// ------------------------------------------------------------------ race

/** `SERIES_LABELS`/`seriesLabel` in `race.ts`, ported. */
internal fun seriesLabel(series: String): String =
    if (series == "f1") {
        "F1"
    } else if (series == "indycar") {
        "IndyCar"
    } else {
        series.uppercase()
    }

/** `abbreviate` in `race.ts` — "Monaco Grand Prix" reads as "Monaco GP". */
internal fun abbreviateEventName(name: String): String =
    name.replace(Regex("""\s+Grand Prix$""", RegexOption.IGNORE_CASE), " GP")

/** `countdown` in `race.ts`, ported. */
internal fun countdown(deltaMs: Long): Pair<String, String> {
    val minutes = (deltaMs / 60_000.0).roundToLong()
    if (minutes < 120) return Pair(maxOf(minutes, 0).toString(), "min")
    val hours = (deltaMs / 3_600_000.0).roundToLong()
    if (hours <= 36) return Pair(hours.toString(), "hr")
    val days = (deltaMs / 86_400_000.0).roundToLong()
    return Pair(days.toString(), if (days == 1L) "day" else "days")
}

private fun raceHeadline(
    pane: MobileRankedPane,
    setup: MobileRaceSetup,
    resolved: MobileRaceResolved,
    nowMs: Long,
): String {
    val label = seriesLabel(pane.subjectKey)
    when (pane.answer.answerState) {
        MobilePaneAnswerState.UNBOUND -> return "Not set up"
        // `race.ts`'s two unacquired arms, ported: an unread or unusable
        // binding is about the SETUP, not this series — no series label on
        // those, because none has been established. A bound binding whose
        // view is null is the "never polled" one.
        MobilePaneAnswerState.BOUND_BUT_UNACQUIRED -> return when (setup) {
            MobileRaceSetup.UNREAD -> "Checking setup"
            MobileRaceSetup.UNUSABLE -> "Setup needs a look"
            MobileRaceSetup.BOUND, MobileRaceSetup.UNSET -> "$label · Never polled"
        }
        MobilePaneAnswerState.ANSWERED -> Unit
    }
    return when (resolved) {
        is MobileRaceResolved.Gap -> "$label · Never polled"
        is MobileRaceResolved.Facts -> {
            val event = resolved.facts.event
            if (event == null) {
                "$label · No races scheduled"
            } else {
                val (value, unit) = countdown(event.startsAtMs - nowMs)
                "$label · ${abbreviateEventName(event.name)} in $value $unit"
            }
        }
    }
}

private fun raceGlyphs(
    pane: MobileRankedPane,
    setup: MobileRaceSetup,
    resolved: MobileRaceResolved,
): List<PaneGlyph> {
    when (pane.answer.answerState) {
        MobilePaneAnswerState.UNBOUND -> return listOf(NOT_SET_UP)
        MobilePaneAnswerState.BOUND_BUT_UNACQUIRED -> return listOf(
            when (setup) {
                MobileRaceSetup.UNREAD -> CHECKING_SETUP
                MobileRaceSetup.UNUSABLE -> SETUP_NEEDS_A_LOOK
                MobileRaceSetup.BOUND, MobileRaceSetup.UNSET ->
                    PaneGlyph.Icon(R.drawable.ic_cloud_fog, "never polled")
            },
        )
        MobilePaneAnswerState.ANSWERED -> Unit
    }
    return when (resolved) {
        is MobileRaceResolved.Gap -> listOf(PaneGlyph.Icon(R.drawable.ic_cloud_fog, "never polled"))
        is MobileRaceResolved.Facts ->
            if (resolved.facts.event == null || resolved.facts.nextStart == null) {
                listOf(PaneGlyph.Icon(R.drawable.ic_flag, "no races scheduled"))
            } else if (resolved.facts.hasLiveAlert) {
                listOf(PaneGlyph.Icon(R.drawable.ic_siren, "starting soon"))
            } else {
                listOf(PaneGlyph.Icon(R.drawable.ic_flag, "next race"))
            }
    }
}

// ------------------------------------------------------------------ kimi

/** `formatUsd` in `kimi.ts`, ported — the sign in front of the symbol
 * (`-$1.00`, never `$-1.00`). */
internal fun formatUsd(amount: Double): String {
    val sign = if (amount < 0) "-" else ""
    return "$sign$${String.format(Locale.US, "%.2f", abs(amount))}"
}

/** `kimiCollapsedHeadline` in `kimi.ts` — the amount, and the decision the
 * band already made about it. The web switches on the core's `kimi_band`;
 * this reads the pane's DECIDED band, which is the same value having
 * crossed the seam once instead of being recomputed here. */
internal fun kimiCollapsedHeadline(availableBalance: Double, band: MobilePaneBand): String {
    val amount = formatUsd(availableBalance)
    return when (band) {
        MobilePaneBand.LIVE -> "$amount — exhausted"
        MobilePaneBand.IMMINENT -> "$amount — critical"
        MobilePaneBand.NEAR -> "$amount — running low"
        MobilePaneBand.DISTANT -> "$amount left"
        MobilePaneBand.DORMANT -> "$amount left"
    }
}

private fun kimiHeadline(pane: MobileRankedPane, resolved: MobileKimiResolved): String {
    if (!answered(pane)) return "No answer yet"
    return when (resolved) {
        is MobileKimiResolved.Gap -> "No answer yet"
        is MobileKimiResolved.Facts ->
            kimiCollapsedHeadline(resolved.facts.availableBalance, pane.answer.band)
    }
}

private fun kimiGlyphs(pane: MobileRankedPane): List<PaneGlyph> {
    if (!answered(pane)) return listOf(NO_ANSWER)
    return when (pane.answer.band) {
        MobilePaneBand.LIVE -> listOf(PaneGlyph.Icon(R.drawable.ic_siren, "kimi balance exhausted"))
        MobilePaneBand.IMMINENT -> listOf(PaneGlyph.Icon(R.drawable.ic_siren, "kimi balance critical"))
        MobilePaneBand.NEAR -> listOf(PaneGlyph.Icon(R.drawable.ic_database, "kimi balance"))
        MobilePaneBand.DISTANT -> listOf(PaneGlyph.Icon(R.drawable.ic_database, "kimi balance"))
        MobilePaneBand.DORMANT -> listOf(PaneGlyph.Icon(R.drawable.ic_database, "kimi balance"))
    }
}

// ---------------------------------------------------------------- github

/** `githubCollapsedHeadline` in `github.ts`, on the decided band — see the
 * file header for the one deliberate deviation (an imminent-and-stale pane
 * reads as stale rather than recomputing the raw band to split the two). */
private fun githubHeadline(pane: MobileRankedPane, resolved: MobileWorkflowResolved, nowMs: Long): String {
    if (!answered(pane)) return "No answer yet"
    return when (resolved) {
        is MobileWorkflowResolved.Gap -> "No answer yet"
        is MobileWorkflowResolved.View -> {
            val view = resolved.view
            val body = view.body
            when (pane.answer.band) {
                MobilePaneBand.LIVE -> "${body.displayName} · never run"
                MobilePaneBand.IMMINENT ->
                    if (view.stale) {
                        "${body.displayName} · answer may be stale, last heard ${heardAgo(view.freshness)}"
                    } else {
                        val lastOk = body.lastScheduledSuccessAtMs
                        if (lastOk == null) {
                            "${body.displayName} · no scheduled success"
                        } else {
                            "${body.displayName} · stalled, last ok ${ageWords(nowMs - lastOk)}"
                        }
                    }
                MobilePaneBand.NEAR -> "${body.displayName} · last run failed"
                MobilePaneBand.DISTANT -> {
                    val lastOk = body.lastScheduledSuccessAtMs ?: nowMs
                    "${body.displayName} · cadence unreadable, last scheduled success ${ageWords(nowMs - lastOk)}"
                }
                MobilePaneBand.DORMANT -> "${body.displayName} · healthy"
            }
        }
    }
}

private fun githubGlyphs(pane: MobileRankedPane, resolved: MobileWorkflowResolved, nowMs: Long): List<PaneGlyph> {
    if (!answered(pane)) return listOf(NO_ANSWER)
    return when (resolved) {
        is MobileWorkflowResolved.Gap -> listOf(NO_ANSWER)
        is MobileWorkflowResolved.View -> {
            val name = resolved.view.body.displayName
            when (pane.answer.band) {
                MobilePaneBand.LIVE -> listOf(PaneGlyph.Icon(R.drawable.ic_siren, "$name never run"))
                MobilePaneBand.IMMINENT ->
                    if (resolved.view.stale) {
                        listOf(PaneGlyph.Icon(R.drawable.ic_cloud_fog, "$name answer may be stale"))
                    } else {
                        listOf(PaneGlyph.Icon(R.drawable.ic_siren, "$name stalled"))
                    }
                MobilePaneBand.NEAR -> listOf(PaneGlyph.Icon(R.drawable.ic_bell, "$name last run failed"))
                MobilePaneBand.DISTANT ->
                    listOf(PaneGlyph.Icon(R.drawable.ic_help_circle, "$name cadence unreadable"))
                MobilePaneBand.DORMANT ->
                    listOf(PaneGlyph.Icon(R.drawable.ic_circle_check, "$name healthy"))
            }
        }
    }
}

// ---------------------------------------------------------------- uptime

/** `uptimeCollapsedHeadline` in `uptime.ts`, ported — reads the body only,
 * no band. */
internal fun uptimeCollapsedHeadline(serviceId: String, body: MobileProbeBody): String {
    when (body.expected) {
        MobileProbeExpected.OFF -> return if (body.error != null) {
            "$serviceId · off, as expected"
        } else {
            "$serviceId · reachable — expected off"
        }
        MobileProbeExpected.ON -> Unit
    }
    val error = body.error
    if (error != null) return "$serviceId · unreachable — $error"
    if (body.observedStatus != body.expectStatus) {
        return "$serviceId · unexpected status ${body.observedStatus} (wanted ${body.expectStatus})"
    }
    return "$serviceId · ${body.observedStatus} as expected"
}

private fun uptimeHeadline(pane: MobileRankedPane, resolved: MobileProbeResolved): String {
    if (!answered(pane)) return "No answer yet"
    return when (resolved) {
        is MobileProbeResolved.Gap -> "No answer yet"
        is MobileProbeResolved.Facts -> {
            val facts = resolved.facts
            // The escalated-stale reading (`uptime.ts`'s own): imminence
            // for this question only ever comes from staleness.
            if (facts.stale && pane.answer.band == MobilePaneBand.IMMINENT) {
                "${facts.serviceId} · answer may be stale, last heard ${heardAgo(facts.freshness)}"
            } else {
                uptimeCollapsedHeadline(facts.serviceId, facts.body)
            }
        }
    }
}

private fun uptimeGlyphs(pane: MobileRankedPane, resolved: MobileProbeResolved): List<PaneGlyph> {
    if (!answered(pane)) return listOf(NO_ANSWER)
    return when (resolved) {
        is MobileProbeResolved.Gap -> listOf(NO_ANSWER)
        is MobileProbeResolved.Facts -> {
            val id = resolved.facts.serviceId
            if (resolved.facts.stale && pane.answer.band == MobilePaneBand.IMMINENT) {
                return listOf(PaneGlyph.Icon(R.drawable.ic_cloud_fog, "$id answer may be stale"))
            }
            when (pane.answer.band) {
                MobilePaneBand.LIVE -> listOf(PaneGlyph.Icon(R.drawable.ic_siren, "$id divergent"))
                MobilePaneBand.NEAR -> listOf(PaneGlyph.Icon(R.drawable.ic_bell, "$id unexpected status"))
                MobilePaneBand.IMMINENT -> listOf(PaneGlyph.Icon(R.drawable.ic_circle_check, "$id as expected"))
                MobilePaneBand.DISTANT -> listOf(PaneGlyph.Icon(R.drawable.ic_circle_check, "$id as expected"))
                MobilePaneBand.DORMANT -> listOf(PaneGlyph.Icon(R.drawable.ic_circle_check, "$id as expected"))
            }
        }
    }
}

// ---------------------------------------------------------- reachability

/** The reachability facts this file needs, decoupled from the uniffi
 * record so [relativeAge]'s port stays testable without the seam. */
internal data class ReachabilityWords(
    val ageMs: Long,
    val stale: Boolean,
    val latestAttemptLanded: Boolean,
)

/** `relative_age` in `hummingbird-core::decisions::settings`, ported — the
 * web reads it through the wasm seam (`shell/sync-status.ts`), which the
 * mobile seam does not export; the wording is pinned against the Rust
 * original by `PaneAnswersTest`. */
internal fun relativeAge(ageMs: Long): String {
    val clamped = maxOf(ageMs, 0)
    val minutes = clamped / 60_000
    if (minutes < 1) return "just now"
    if (minutes < 60) return "${minutes}m ago"
    val hours = minutes / 60
    if (hours < 24) return "${hours}h ago"
    return "${hours / 24}d ago"
}

internal fun reachabilityHeadline(words: ReachabilityWords?): String {
    if (words == null) return "Never synced on this device."
    val verb = if (words.latestAttemptLanded) "Synced" else "Last synced"
    return "$verb ${relativeAge(words.ageMs)}"
}

private fun reachabilityGlyphs(stale: Boolean?): List<PaneGlyph> = when (stale) {
    null -> listOf(PaneGlyph.Icon(R.drawable.ic_cloud_fog, "never synced on this device"))
    true -> listOf(PaneGlyph.Icon(R.drawable.ic_siren, "authority sync stale"))
    false -> listOf(PaneGlyph.Icon(R.drawable.ic_circle_check, "authority recently reached"))
}
