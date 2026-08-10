//! Throwaway civil-date support. Not part of the portable logic — the real
//! adapter will use whatever date type the server settles on. Days since the
//! Unix epoch, Howard Hinnant's civil-from-days algorithms.

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Date(i64);

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

    pub fn short(self) -> String {
        format!("{} {}", &self.weekday()[..3], self.iso())
    }

    /// Local midnight, as epoch seconds. The prototype pretends the city and
    /// the device share one timezone; a real adapter must not.
    pub fn epoch_start(self) -> i64 {
        self.0 * 86_400
    }

    /// End of this calendar day — what ADR-0014's `city-waste` entry means by
    /// "end of the affected collection date".
    pub fn epoch_end(self) -> i64 {
        (self.0 + 1) * 86_400
    }
}
