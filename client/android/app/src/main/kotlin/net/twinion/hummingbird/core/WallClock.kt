package net.twinion.hummingbird.core

import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.Locale

/** The one place this app reads a timezone.
 *
 * Every decision in `hummingbird-core` takes "now" as an already-resolved
 * civil string in the deadline grammar's own shape (`YYYY-MM-DDTHH:MM`),
 * because that crate holds no timezone table — deliberately, and at length
 * (`client/core/Cargo.toml`; ADR-0015's "resolves no civil date to an
 * instant, the reader does, in its own zone"). Turning an epoch
 * millisecond count into that shape is therefore the host's job, and this
 * is the host's whole share of it. The web's equivalent is
 * `decisions/seam.ts`'s `localWallClock`/`utcWallClock`; these two must
 * mean the same things.
 *
 * **Two readings, not one.** The rules backtest reads one instant in two
 * frames: `deadline` and `scheduled_date` are device-local civil strings,
 * while `occurred_at` is stamped UTC by the authority
 * (`hummingbird_domain::now_as_deadline`'s own doc: "the Worker runs UTC.
 * Not per-rule, not per-device"). Naming both at the seam is what lets the
 * core compare each field in the frame it belongs to without ever
 * resolving a zone itself.
 *
 * Seconds are truncated, never rounded, in both — the deadline grammar is
 * minute-precision and `now_as_deadline` truncates.
 */
object WallClock {

    private val SHAPE: DateTimeFormatter = DateTimeFormatter.ofPattern("uuuu-MM-dd'T'HH:mm")

    /** This device's own wall-clock reading of `nowMs`. */
    fun local(nowMs: Long, zone: ZoneId = ZoneId.systemDefault()): String =
        Instant.ofEpochMilli(nowMs).atZone(zone).format(SHAPE)

    /** The same instant in UTC — the frame `occurred_at` is stamped in. */
    fun utc(nowMs: Long): String =
        Instant.ofEpochMilli(nowMs).atZone(ZoneOffset.UTC).format(SHAPE)

    // ------------------------------------------------- the picker boundary
    //
    // Material 3's `DatePickerState` speaks epoch milliseconds and the wire
    // speaks a naive civil date, so `ui/forms/CaptureDateField` needs a
    // conversion in both directions. It lives here rather than inside the
    // composable for the reason this file's header states: ADR-0025's
    // verdict table puts the epoch/civil conversion per-client on purpose
    // (the core holds no tzdb), and "per-client" means one place in the
    // client, not one per control.
    //
    // **`ZoneOffset.UTC`, never `ZoneId.systemDefault()`, and the choice is
    // load-bearing.** `DatePickerState.selectedDateMillis` is UTC midnight
    // of the picked civil day by Material's own contract — it is a calendar
    // day wearing an instant's clothes, not an instant. Resolving it in the
    // device zone shifts the picked day by one for every reader west of
    // Greenwich, and the defect is invisible to anyone testing at midday.
    // The two functions below are inverses only because they agree on this.

    private val DATE_SHAPE: DateTimeFormatter = DateTimeFormatter.ISO_LOCAL_DATE

    /** The civil day a date picker's `selectedDateMillis` names, as
     * `YYYY-MM-DD`. */
    fun civilDate(utcMs: Long): String =
        Instant.ofEpochMilli(utcMs).atZone(ZoneOffset.UTC).toLocalDate().format(DATE_SHAPE)

    /** The inverse: where a date picker should open when the field already
     * holds [date]. `null` when [date] is not a civil date **this picker can
     * show** — which leaves it on its own default of today rather than
     * refusing to open. Deciding whether such a value may be *sent* is the
     * core's job (`captureMetaProblems`), not this function's.
     *
     * Three kinds of value answer `null`, and the third is the one that bit:
     * the empty field; the legacy free text `split_deadline` deliberately
     * passes through; and **a real, valid date whose year is outside
     * [years]**. Material's `DatePickerState` does not clamp an out-of-range
     * year, it `require`s it — `IllegalArgumentException: The initial display
     * month's year (2206) is out of the years range of 1900..2100` — so
     * handing one to `rememberDatePickerState` crashes the surface. The core
     * bounds no year (`server/domain/src/deadline.rs` checks only that the
     * calendar date exists), so `2206-08-15` is a perfectly saveable
     * deadline, and a fat-fingered one is exactly the value a reader most
     * needs to open the picker to fix.
     *
     * [years] is the caller's, not a constant here: the authority is
     * `DatePickerDefaults.YearRange`, and that lives in Material. Passing it
     * in keeps this file free of a Compose dependency and stops the two
     * bounds drifting apart.
     *
     * The year check runs **before** the conversion, which is also what makes
     * the arithmetic safe: `ISO_LOCAL_DATE` accepts a sign-prefixed wide year
     * (`+999999999-12-31`), and `toEpochMilli` overflows on one. */
    fun civilDateMillis(date: String, years: IntRange): Long? =
        try {
            val parsed = LocalDate.parse(date, DATE_SHAPE)
            if (parsed.year !in years) {
                null
            } else {
                parsed.atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli()
            }
        } catch (notADate: java.time.format.DateTimeParseException) {
            null
        }

    /** A time picker's reading, as the `HH:MM` half of a deadline. No zone
     * is involved: the picker reports the hour and minute a reader chose,
     * and the deadline grammar stores exactly that.
     *
     * **`Locale.ROOT` is load-bearing.** `"%02d".format(9)` resolves against
     * the default locale, and a locale whose numbering system is not Latin
     * renders it in that system's digits — `٠٩:٣٠` under `ar-EG`, and the
     * same for `fa`, `bn` and `my`. `hummingbird_domain::is_valid_deadline`
     * accepts ASCII digits only, so the deadline would be refused with "Use
     * YYYY-MM-DD or YYYY-MM-DDTHH:MM" and the reader would have no way to
     * correct it: the field is read-only. `WallClockPickerTest` pins it in
     * four such locales. The `DateTimeFormatter`s above are already immune
     * (`DecimalStyle.STANDARD` is ASCII regardless of locale). */
    fun civilTime(hour: Int, minute: Int): String =
        String.format(Locale.ROOT, "%02d:%02d", hour, minute)

    /** The inverse of [civilTime]: where a time picker should open when the
     * deadline already names a minute. `null` when [time] is not one this
     * can resolve, which sends the caller to [currentHourMinute]. */
    fun hourMinute(time: String): Pair<Int, Int>? =
        try {
            LocalTime.parse(time).let { it.hour to it.minute }
        } catch (notATime: java.time.format.DateTimeParseException) {
            null
        }

    /** Where a time picker should open when the deadline names no minute
     * yet — this device's current hour and minute. The alternative,
     * seeding at `00:00`, would be a silent edit: a date-only deadline
     * means *end* of that day (`server/domain/src/deadline.rs`), so
     * `T00:00` moves it almost a full day earlier. */
    fun currentHourMinute(zone: ZoneId = ZoneId.systemDefault()): Pair<Int, Int> =
        LocalTime.now(zone).let { it.hour to it.minute }
}
