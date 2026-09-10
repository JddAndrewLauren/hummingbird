package net.twinion.hummingbird.core

import java.util.Locale
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

/** The picker boundary's own tests — the five functions `ui/forms/`'s two
 * date controls convert through.
 *
 * `CaptureFieldSetStructuralTest` pins that these resolve in `ZoneOffset.UTC`,
 * but a source-text pin can only see that the token is present; it cannot see
 * an inverted conversion or a locale-dependent format. These can.
 */
class WallClockPickerTest {

    /** `DatePickerDefaults.YearRange`, restated so this suite needs no
     * Compose dependency. `CaptureDateField` passes the real one. */
    private val PICKER_YEARS = 1900..2100

    private val original = Locale.getDefault()

    @After
    fun restoreLocale() = Locale.setDefault(original)

    /** `DatePickerState.selectedDateMillis` is UTC midnight of the picked
     * civil day by Material's contract. The value below is UTC midnight on
     * 1 August 2026 — which in any zone west of Greenwich is still 31 July
     * locally, so a `ZoneId.systemDefault()` read returns the wrong day.
     * Pinned with the default zone forced west precisely so the wrong
     * implementation fails here rather than on somebody's phone. */
    @Test
    fun `a picked civil day is read in UTC, not the device zone`() {
        val utcMidnightAug1 = 1785542400000L // 2026-08-01T00:00:00Z
        val previous = System.getProperty("user.timezone")
        try {
            java.util.TimeZone.setDefault(java.util.TimeZone.getTimeZone("America/Los_Angeles"))
            assertEquals("2026-08-01", WallClock.civilDate(utcMidnightAug1))
        } finally {
            java.util.TimeZone.setDefault(java.util.TimeZone.getTimeZone(previous ?: "UTC"))
        }
    }

    @Test
    fun `civilDate and civilDateMillis are inverses`() {
        for (date in listOf("2026-01-01", "2026-08-24", "2026-12-31", "2028-02-29")) {
            val ms = WallClock.civilDateMillis(date, PICKER_YEARS)
                ?: error("$date should resolve")
            assertEquals(date, WallClock.civilDate(ms))
        }
    }

    /** Empty, free text, and an impossible calendar date all resolve to
     * `null` — which opens the picker on today rather than refusing. Whether
     * such a value may be *sent* is the core's question, not this one's. */
    @Test
    fun `a value the picker cannot represent resolves to null rather than throwing`() {
        for (bad in listOf("", "   ", "next tuesday", "2026-02-30", "abcd-ef-gh", "2026-8-1")) {
            assertNull("$bad should not resolve", WallClock.civilDateMillis(bad, PICKER_YEARS))
        }
    }

    /** The crash this guard exists for. Material's `DatePickerState` does not
     * clamp a year outside its range, it `require`s it — verified against
     * material3 1.3.x: `rememberDatePickerState(initialSelectedDateMillis =`
     * a 2206 date`)` throws `IllegalArgumentException: The initial display
     * month's year (2206) is out of the years range of 1900..2100`.
     *
     * Nothing upstream prevents such a value: `is_valid_deadline` bounds only
     * the calendar, so `2206-08-15` is a real, saveable, syncable deadline —
     * and a mistyped year is exactly the value a reader most needs to open
     * the picker to correct. It must resolve to `null` (open on today), never
     * to a Long the picker will then reject. */
    @Test
    fun `a valid date outside the picker's year range resolves to null, not a crash`() {
        for (outside in listOf("2206-08-15", "1899-12-31", "1066-10-14", "0001-01-01")) {
            assertNull("$outside is outside $PICKER_YEARS", WallClock.civilDateMillis(outside, PICKER_YEARS))
        }
        // The edges themselves are inside and must still resolve.
        for (edge in listOf("1900-01-01", "2100-12-31")) {
            assertNotNull("$edge is inside $PICKER_YEARS", WallClock.civilDateMillis(edge, PICKER_YEARS))
        }
    }

    /** `ISO_LOCAL_DATE` accepts a sign-prefixed wide year, and converting one
     * to epoch millis overflows. The year guard runs first, so the
     * `ArithmeticException` is unreachable rather than caught — this pins
     * that ordering, since swapping it back reintroduces a crash the
     * `DateTimeParseException` catch would not stop. */
    @Test
    fun `a wide year is refused before it can overflow the conversion`() {
        assertNull(WallClock.civilDateMillis("+999999999-12-31", PICKER_YEARS))
        assertNull(WallClock.civilDateMillis("-999999999-01-01", PICKER_YEARS))
    }

    /** The bug this test exists for: `String.format("%02d:%02d", …)` uses the
     * default locale, and locales whose numbering system is not Latin render
     * `09:30` as `٠٩:٣٠`. That string is refused by
     * `hummingbird_domain::is_valid_deadline`, so a reader on an Arabic,
     * Persian, Bengali or Burmese device could pick a time and then be unable
     * to submit — with no way to correct it, the field being read-only. */
    @Test
    fun `a named minute is ASCII in every locale`() {
        for (tag in listOf("en-US", "ar-EG-u-nu-arab", "fa-IR", "bn-IN-u-nu-beng", "my-MM")) {
            Locale.setDefault(Locale.forLanguageTag(tag))
            assertEquals("under $tag", "09:30", WallClock.civilTime(9, 30))
            assertEquals("under $tag", "00:00", WallClock.civilTime(0, 0))
            assertEquals("under $tag", "23:59", WallClock.civilTime(23, 59))
        }
    }

    @Test
    fun `civilTime and hourMinute are inverses`() {
        for ((h, m) in listOf(0 to 0, 9 to 30, 12 to 5, 23 to 59)) {
            assertEquals(h to m, WallClock.hourMinute(WallClock.civilTime(h, m)))
        }
    }

    @Test
    fun `a time the picker cannot represent resolves to null rather than throwing`() {
        for (bad in listOf("", "9:3", "half past nine", "25:00", "09:60")) {
            assertNull("$bad should not resolve", WallClock.hourMinute(bad))
        }
    }
}
