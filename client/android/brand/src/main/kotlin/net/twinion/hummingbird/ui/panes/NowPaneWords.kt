package net.twinion.hummingbird.ui.panes

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale
import uniffi.hummingbird_ffi_mobile.MobileRaceGap
import uniffi.hummingbird_ffi_mobile.MobileScpsEvent
import uniffi.hummingbird_ffi_mobile.MobileScpsKind
import uniffi.hummingbird_ffi_mobile.MobileScpsQuestFact
import uniffi.hummingbird_ffi_mobile.MobileWasteGap
import uniffi.hummingbird_ffi_mobile.MobileWeekendEntry
import uniffi.hummingbird_ffi_mobile.MobileWeekendEntryAnchor
import uniffi.hummingbird_ffi_mobile.MobileWeekendEntryKind
import uniffi.hummingbird_ffi_mobile.MobileWeekendGap

// The Now surface's pure word helpers — every sentence, label and clock the
// expanded cards say, with no Compose in them. They were `NowPanesExpanded.kt`'s
// private half until ADR-0039: the watch renders the same six Now questions
// and must say exactly what the phone says, so the words moved into `:brand`
// where both device apps read them, and the phone's Material3 composables
// stayed behind in `:app`. Each helper's own doc names the web function it
// ports; the rules about what a word may and may not decide are
// `NowPanesExpanded.kt`'s header, and `PaneContentStructuralTest` still
// gates them (no second clock, exhaustive `when`s with no `else`).
//
// `WEEKDAYS` comes from `PaneAnswers.kt`, same package, same module.

// ------------------------------------------------------------------ scps

/** `KIND_LABEL` in `scps.ts`, ported — the contract's own rule that an
 * unrecognised `SCPS ` title reads as the literal lowercase word "event". */
private val SCPS_KIND_LABEL: Map<MobileScpsKind, String> = mapOf(
    MobileScpsKind.MEETING to "Meeting",
    MobileScpsKind.ACTIVITY to "Activity",
    MobileScpsKind.HAPPY_HOUR to "Happy Hour",
    MobileScpsKind.EVENT to "event",
)

private val SCPS_WEEKDAY_NAMES = listOf("Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat")

private val SCPS_MONTH_NAMES = listOf(
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
)

/** `monthName` in `scps.ts`, ported — `"January"` for `"2026-09"`'s `09`. */
fun scpsMonthName(monthToken: String): String {
    val month = monthToken.drop(5).take(2).toIntOrNull()
    return SCPS_MONTH_NAMES.getOrNull((month ?: 0) - 1) ?: monthToken
}

/** `scpsTimeLabel` in `scps.ts`, ported — local wall time at the device's
 * own zone (the same one `ScpsEvent.startMs` already resolved against),
 * `"9:00am"`: no space, lowercase, never the localized `DateTimeFormatter`
 * spelling `raceClock` uses for the race card. */
fun scpsTimeLabel(atMs: Long, zone: ZoneId = ZoneId.systemDefault()): String {
    val at = Instant.ofEpochMilli(atMs).atZone(zone)
    val hour24 = at.hour
    val minutes = at.minute.toString().padStart(2, '0')
    val hour12 = if (hour24 % 12 == 0) 12 else hour24 % 12
    val ampm = if (hour24 >= 12) "pm" else "am"
    return "$hour12:$minutes$ampm"
}

/** `scpsDayLabel` in `scps.ts`, ported — "today" · "tomorrow" · "Sat" ·
 * "9 Mar", off the event's own `daysUntil`/`startDate`, never the device
 * clock recomputed here. */
fun scpsDayLabel(event: MobileScpsEvent): String {
    if (event.inProgress || event.daysUntil <= 0L) return "today"
    if (event.daysUntil == 1L) return "tomorrow"
    if (event.daysUntil < 7L) {
        val weekday = LocalDate.parse(event.startDate).dayOfWeek.value % 7
        return SCPS_WEEKDAY_NAMES[weekday]
    }
    val (_, month, day) = event.startDate.split("-")
    val abbrev = SCPS_MONTH_NAMES.getOrNull(month.toInt() - 1)?.take(3) ?: month
    return "${day.toInt()} $abbrev"
}

/** `scpsCardTitle` in `scps.ts`, ported — kind and topic only, no day or
 * time: the meta line beneath this title owns those (the file header's own
 * rule). Only the collapsed row's `scpsEventHeadline` (`PaneAnswers.kt`)
 * carries the full sentence. */
fun scpsCardTitle(event: MobileScpsEvent): String {
    if (event.kind == MobileScpsKind.HAPPY_HOUR && event.inProgress) {
        return "SCPS Happy Hour in progress"
    }
    val topic = if (!event.topic.isNullOrBlank()) " — ${event.topic}" else ""
    return "SCPS ${SCPS_KIND_LABEL.getValue(event.kind)}$topic"
}

/** `scpsQuestLine` in `scps.ts`, ported — the current month's phrase, the
 * last-posted one named against its own month, the malformed-value line
 * (#702), or the plain "unset" line. */
fun scpsQuestLine(quest: MobileScpsQuestFact, nowMs: Long, zone: ZoneId = ZoneId.systemDefault()): String =
    when (quest) {
        is MobileScpsQuestFact.Current -> "Photo Quest — ${quest.phrase}"
        is MobileScpsQuestFact.Other -> {
            val today = Instant.ofEpochMilli(nowMs).atZone(zone).toLocalDate()
            val currentMonth = scpsMonthName(String.format(Locale.US, "%04d-%02d", today.year, today.monthValue))
            "No quest posted for $currentMonth; last: ${quest.phrase} (${scpsMonthName(quest.month)})"
        }
        is MobileScpsQuestFact.Malformed -> "Quest not understood: \"${quest.text}\" — expected \"YYYY-MM phrase\""
        MobileScpsQuestFact.None -> "No quest set"
    }

// ----------------------------------------------------------------- waste

/** `wasteHeadline` in `waste.ts` (the EXPANDED card's sentence — wordier
 * than the collapsed `wasteCollapsedHeadline`), ported verbatim, capital
 * letters included. */
fun wasteExpandedHeadline(daysAway: Long, weekday: String, holiday: Boolean): String {
    if (daysAway == 0L) return "Trash Today"
    if (holiday) return "Trash $weekday"
    if (daysAway == 1L) return "Trash Tonight"
    return "Trash $weekday"
}

/** `gapReason` in `waste.ts`, ported per kind. */
fun wasteGapReason(gap: MobileWasteGap): String = when (gap) {
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

// ------------------------------------------------------------------ race

/** `gapReason` in `race.ts`, ported per kind. */
fun raceGapReason(gap: MobileRaceGap): String = when (gap) {
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
fun raceDayLabel(atMs: Long, nowMs: Long, zone: ZoneId = ZoneId.systemDefault()): String {
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
fun raceClock(atMs: Long, zone: ZoneId = ZoneId.systemDefault()): String =
    Instant.ofEpochMilli(atMs)
        .atZone(zone)
        .format(DateTimeFormatter.ofPattern("h:mm a", Locale.US))

// --------------------------------------------------------------- weekend

/** Why an answered-looking weekend pane has nothing to draw, per gap kind
 * — the expanded card's own sentence, wordier than the collapsed
 * headline's "Checking calendar", exactly as `wasteGapReason` is wordier
 * than its headline. Exhaustive with no `else`: a fourth gap in the core's
 * enum is a compile error here, not a card that renders as nothing. */
fun weekendGapReason(gap: MobileWeekendGap): String = when (gap) {
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
fun shortDayLabel(dayKey: String): String = runCatching {
    java.time.LocalDate.parse(dayKey).format(DateTimeFormatter.ofPattern("EEE", Locale.getDefault()))
}.getOrDefault(dayKey)

/** One entry's time words — `weekend.ts`'s `timeLabel`, ported.
 *
 * Reads the entry's own `anchor` rather than inferring one from the
 * timestamp: whether something covers a day or sits at an instant is the
 * core's answer (`weekend.rs`'s `EntryAnchor`), and a Kotlin
 * `atMs % DAY_MS == 0` test would be a second, worse copy of it. */
fun weekendEntryTimeLabel(entry: MobileWeekendEntry, zone: ZoneId): String =
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
fun plannedDayOf(entry: MobileWeekendEntry): String? = when (entry.kind) {
    MobileWeekendEntryKind.SCHEDULED -> entry.dayKey
    MobileWeekendEntryKind.DUE -> entry.alsoScheduledOn
    MobileWeekendEntryKind.EVENT -> null
}

/** Whether a plan chip is offered at all: only for something with an item
 * behind it. An event is the calendar's, and this app writes no calendar
 * (ADR-0002 rule 1 — a context mirror cannot mint or modify anything). */
fun offersPlanChips(entry: MobileWeekendEntry): Boolean =
    entry.kind != MobileWeekendEntryKind.EVENT
