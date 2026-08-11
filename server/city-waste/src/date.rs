//! Civil dates: whole days, no time of day, no zone. Days since the Unix
//! epoch, via Howard Hinnant's civil-from-days algorithms — lifted from the
//! `waste-cadence` prototype, which is deleted with this slice.
//!
//! **Why not a date library for this half.** Everything the cadence and the
//! judgement do is integer arithmetic on day numbers: "which Monday is this",
//! "how many days later did it actually happen", "how many periods ahead".
//! Expressed as day counts those are one subtraction each and impossible to
//! get subtly wrong; expressed as calendar values they need a library and
//! invite a time zone into arithmetic that has no business knowing about one.
//!
//! **Where the zone does belong, and the one thing this module delegates.**
//! An alert's `expires_at` is an *instant*, and ADR-0014 wants it at the end
//! of the collection day **at the address**, not at whatever offset the CI
//! runner happens to sit at. A day boundary in an IANA zone cannot be derived
//! from a day number, so [`Date::end_of_day_ms`] hands that one question to
//! `jiff`. That is the same seam the client draws: `zoned-day.ts` is its own
//! module using `Intl`, and every day-shaped question on the pane goes
//! through it.

use std::fmt;

/// A civil date, as days since 1970-01-01.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Date(i64);

/// Why a date string could not be read. Named rather than an `Option` so a
/// page parse failure reaches the operator's log as something specific.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DateError {
    /// Not `YYYY-MM-DD`.
    NotIso(String),
    /// Well-shaped but not a real day (`2026-02-30`, `2026-13-01`).
    NotACalendarDay(String),
}

impl fmt::Display for DateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DateError::NotIso(s) => write!(f, "`{s}` is not a YYYY-MM-DD date"),
            DateError::NotACalendarDay(s) => write!(f, "`{s}` is not a real calendar day"),
        }
    }
}

impl Date {
    pub fn ymd(y: i64, m: i64, d: i64) -> Date {
        let y = if m <= 2 { y - 1 } else { y };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        Date(era * 146097 + doe - 719468)
    }

    /// Parses `YYYY-MM-DD`, and **round-trips to reject a non-day**:
    /// [`Date::ymd`] is a pure formula that happily maps 2026-02-30 onto
    /// March 2nd, so the only way to tell a real day from a normalised one
    /// is to render the result back and compare.
    pub fn parse(text: &str) -> Result<Date, DateError> {
        let malformed = || DateError::NotIso(text.to_string());
        let (y, rest) = text.split_once('-').ok_or_else(malformed)?;
        let (m, d) = rest.split_once('-').ok_or_else(malformed)?;
        if y.len() != 4 || m.len() != 2 || d.len() != 2 {
            return Err(malformed());
        }
        let y: i64 = y.parse().map_err(|_| malformed())?;
        let m: i64 = m.parse().map_err(|_| malformed())?;
        let d: i64 = d.parse().map_err(|_| malformed())?;
        let date = Date::ymd(y, m, d);
        if date.iso() != text {
            return Err(DateError::NotACalendarDay(text.to_string()));
        }
        Ok(date)
    }

    pub fn from_days(n: i64) -> Date {
        Date(n)
    }

    pub fn days(self) -> i64 {
        self.0
    }

    pub fn add_days(self, n: i64) -> Date {
        Date(self.0 + n)
    }

    pub fn to_ymd(self) -> (i64, i64, i64) {
        let z = self.0 + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        (if m <= 2 { y + 1 } else { y }, m, d)
    }

    pub fn iso(self) -> String {
        let (y, m, d) = self.to_ymd();
        format!("{y:04}-{m:02}-{d:02}")
    }

    pub fn weekday(self) -> &'static str {
        // 1970-01-01 was a Thursday.
        const NAMES: [&str; 7] = [
            "Thursday",
            "Friday",
            "Saturday",
            "Sunday",
            "Monday",
            "Tuesday",
            "Wednesday",
        ];
        NAMES[self.0.rem_euclid(7) as usize]
    }

    /// The instant this civil day ends **at the address** — the address's
    /// next midnight, as epoch milliseconds.
    ///
    /// This is what ADR-0014's `city-waste/v2` expiry wording resolves to,
    /// and the zone is not optional decoration: an alert about a Tuesday
    /// collection in `America/Los_Angeles` that expired at UTC midnight
    /// would vanish from the pane at 5pm Monday, seven hours before the
    /// evening it exists to warn about.
    ///
    /// `None` for a zone the tzdb has never heard of — a malformed payload,
    /// never a crash, and never a silent fallback to UTC, which would be
    /// exactly the wrong answer dressed as an answer.
    pub fn end_of_day_ms(self, zone: &str) -> Option<i64> {
        let tz = jiff::tz::TimeZone::get(zone).ok()?;
        let (y, m, d) = self.add_days(1).to_ymd();
        let midnight = jiff::civil::date(i16::try_from(y).ok()?, i8::try_from(m).ok()?, i8::try_from(d).ok()?);
        // A civil midnight can be skipped by a DST jump (rare, but real —
        // some zones spring forward at 00:00). `to_zoned` resolves it
        // forward to the first instant that day actually has, which is the
        // reading that keeps "end of the collection day" monotonic.
        Some(midnight.to_zoned(tz).ok()?.timestamp().as_millisecond())
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.iso())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_round_trips_through_days() {
        for text in ["1970-01-01", "2026-08-17", "2000-02-29", "2100-03-01"] {
            assert_eq!(Date::parse(text).unwrap().iso(), text);
        }
    }

    #[test]
    fn weekdays_are_right_for_known_days() {
        assert_eq!(Date::parse("1970-01-01").unwrap().weekday(), "Thursday");
        // 2026-08-17 is the Monday this slice's fixtures are built around.
        assert_eq!(Date::parse("2026-08-17").unwrap().weekday(), "Monday");
        assert_eq!(Date::parse("2026-08-18").unwrap().weekday(), "Tuesday");
    }

    /// The round-trip check earns its keep here: `ymd` is a formula, so
    /// every one of these parses "successfully" into some real day unless
    /// the result is rendered back and compared.
    #[test]
    fn a_well_shaped_non_day_is_rejected_rather_than_normalised() {
        for text in ["2026-02-30", "2026-13-01", "2026-00-10", "2026-01-32"] {
            assert_eq!(
                Date::parse(text),
                Err(DateError::NotACalendarDay(text.to_string())),
                "{text}"
            );
        }
        for text in ["17/08/2026", "2026-8-17", "", "2026-08-17T00:00:00Z"] {
            assert!(matches!(Date::parse(text), Err(DateError::NotIso(_))), "{text}");
        }
    }

    /// The zone is the whole point of delegating this one question. Pinned
    /// against a real offset: 2026-08-18 in Los Angeles is PDT (UTC-7), so
    /// the day ends at 07:00Z on the 19th — seven hours *after* the UTC
    /// midnight a zone-blind implementation would have picked.
    #[test]
    fn end_of_day_is_midnight_at_the_address_not_at_utc() {
        let day = Date::parse("2026-08-18").unwrap();
        let at_address = day.end_of_day_ms("America/Los_Angeles").unwrap();
        let at_utc = day.end_of_day_ms("UTC").unwrap();
        assert_eq!(at_address - at_utc, 7 * 3_600_000, "PDT is UTC-7");
        assert!(at_address > at_utc);
    }

    #[test]
    fn an_unknown_zone_is_none_never_a_silent_utc() {
        assert_eq!(Date::parse("2026-08-18").unwrap().end_of_day_ms("Mars/Olympus"), None);
    }
}
