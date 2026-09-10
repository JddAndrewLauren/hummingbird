package net.twinion.hummingbird.wear.items

import androidx.compose.ui.graphics.Color
import java.time.LocalDate
import java.time.format.DateTimeParseException
import java.time.format.TextStyle
import java.util.Locale
import net.twinion.hummingbird.ui.theme.Ember400
import net.twinion.hummingbird.ui.theme.Ink400
import net.twinion.hummingbird.ui.theme.UrgencyOverdueDark
import net.twinion.hummingbird.ui.theme.UrgencySoonDark
import uniffi.hummingbird_ffi_mobile.MobileUrgencyBand

// The Items row's words and dot (the Wear capture design handoff,
// 2026-09-10) — a rendering of two facts the seam decided, the band
// (`NowItemRecord.urgency`, `compute_urgency`'s answer) and the deadline
// string, plus the day the board was read on. Nothing here decides a band:
// the phone's `NowRow.kt` draws the same band as one word and the raw
// deadline beside it; the watch, with less room, folds the two into one
// mono line — `OVERDUE · THU`, `DUE TODAY`, `DUE FRI`, `DUE OCT 3`,
// `NO DEADLINE`. The weekday is a calendar fact of the `YYYY-MM-DD` string
// itself, so no zone is read here; the one clock is the board's `today`,
// passed in. Pure, and pinned by `ItemRowLabelTest`.

/** The row's first line. [today] is the board's own date (`YYYY-MM-DD`),
 * so "today" is the day the rank was taken on and nothing here reads a
 * clock. */
fun urgencyRowLabel(band: MobileUrgencyBand, deadline: String?, today: String): String {
    val date = deadline?.let(::deadlineDate)
    return when (band) {
        MobileUrgencyBand.OVERDUE -> date?.let { "OVERDUE · ${weekday(it)}" } ?: "OVERDUE"
        MobileUrgencyBand.NOW ->
            if (date == null || date.toString() == today) "DUE TODAY" else "DUE ${weekday(date)}"
        MobileUrgencyBand.SOON -> date?.let { "DUE ${weekday(it)}" } ?: "SOON"
        MobileUrgencyBand.CALM -> date?.let { "DUE ${monthDay(it)}" } ?: "NO DEADLINE"
    }
}

/** The dot beside the label — the phone's `NowRow.urgencyColor` dark arm,
 * with one difference the design asked for: calm is drawn (`--urgency-calm`
 * dark, `Ink400`) rather than omitted, because on this list every row has
 * a dot and a missing one would read as a gap, not as calm. */
fun urgencyDot(band: MobileUrgencyBand): Color = when (band) {
    MobileUrgencyBand.OVERDUE -> UrgencyOverdueDark
    MobileUrgencyBand.NOW -> Ember400
    MobileUrgencyBand.SOON -> UrgencySoonDark
    MobileUrgencyBand.CALM -> Ink400
}

/** The row's third line: the context, then the size when judged —
 * `@PHONE · SIZE:QUICK`; `null` when neither is set, so the row draws two
 * lines rather than an empty third. Raw wire words, upper-cased for the
 * mono register; unjudged stays unsaid (#558's rule). */
fun itemMetaLine(context: String?, size: String?): String? {
    val parts = listOfNotNull(
        context?.takeIf { it.isNotEmpty() }?.uppercase(Locale.ROOT),
        size?.takeIf { it.isNotEmpty() }?.let { "SIZE:${it.uppercase(Locale.ROOT)}" },
    )
    return if (parts.isEmpty()) null else parts.joinToString(" · ")
}

/** The civil day of a deadline in either wire shape (`YYYY-MM-DD` or
 * `YYYY-MM-DDTHH:MM`), or `null` for the legacy free text `split_deadline`
 * passes through — which then reads as the band's word alone. */
private fun deadlineDate(deadline: String): LocalDate? =
    try {
        LocalDate.parse(deadline.take(DATE_LENGTH))
    } catch (notADate: DateTimeParseException) {
        null
    }

private fun weekday(date: LocalDate): String =
    date.dayOfWeek.getDisplayName(TextStyle.SHORT, Locale.ROOT).uppercase(Locale.ROOT)

private fun monthDay(date: LocalDate): String =
    "${date.month.getDisplayName(TextStyle.SHORT, Locale.ROOT).uppercase(Locale.ROOT)} ${date.dayOfMonth}"

private const val DATE_LENGTH = "YYYY-MM-DD".length
