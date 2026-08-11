//! The collection rhythm: an anchor occurrence and a period in weeks.
//!
//! Lifted from the `waste-cadence` prototype, minus the per-stream fan-out —
//! under the corrected domain there is **one** collection day, so there is
//! one cadence, not one per bin.
//!
//! The weekday is deliberately not stored. It is the anchor's weekday, and a
//! second copy of it is a second thing that can disagree with the first.

use crate::date::Date;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cadence {
    pub anchor: Date,
    pub every_n_weeks: i64,
}

impl Cadence {
    pub fn period_days(&self) -> i64 {
        7 * self.every_n_weeks
    }

    /// The first cadence date on or after `d`.
    pub fn next_on_or_after(&self, d: Date) -> Date {
        let p = self.period_days();
        let delta = d.days() - self.anchor.days();
        let mut k = delta.div_euclid(p);
        if delta.rem_euclid(p) != 0 {
            k += 1;
        }
        Date::from_days(self.anchor.days() + k * p)
    }

    /// The last cadence date on or before `d`.
    pub fn latest_on_or_before(&self, d: Date) -> Date {
        let p = self.period_days();
        let delta = d.days() - self.anchor.days();
        Date::from_days(self.anchor.days() + delta.div_euclid(p) * p)
    }

    /// Human words for the rhythm, for an alert body.
    pub fn describe(&self) -> String {
        match self.every_n_weeks {
            1 => format!("every {}", self.anchor.weekday()),
            n => format!("every {n} weeks on {}", self.anchor.weekday()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weekly() -> Cadence {
        // Monday 2026-08-03.
        Cadence { anchor: Date::parse("2026-08-03").unwrap(), every_n_weeks: 1 }
    }

    #[test]
    fn a_cadence_date_is_its_own_neighbour_in_both_directions() {
        let c = weekly();
        let monday = Date::parse("2026-08-17").unwrap();
        assert_eq!(c.latest_on_or_before(monday), monday);
        assert_eq!(c.next_on_or_after(monday), monday);
    }

    #[test]
    fn neighbours_bracket_a_day_between_two_collections() {
        let c = weekly();
        let thursday = Date::parse("2026-08-20").unwrap();
        assert_eq!(c.latest_on_or_before(thursday).iso(), "2026-08-17");
        assert_eq!(c.next_on_or_after(thursday).iso(), "2026-08-24");
    }

    /// Both directions must keep working for days *before* the anchor —
    /// `div_euclid`, not `/`, is what makes the negative side land on the
    /// same lattice rather than truncating toward it.
    #[test]
    fn the_lattice_extends_backwards_past_the_anchor() {
        let c = weekly();
        let before = Date::parse("2026-07-30").unwrap();
        assert_eq!(c.latest_on_or_before(before).iso(), "2026-07-27");
        assert_eq!(c.next_on_or_after(before).iso(), "2026-08-03");
    }

    #[test]
    fn a_fortnightly_cadence_skips_the_intervening_week() {
        let c = Cadence { anchor: Date::parse("2026-08-03").unwrap(), every_n_weeks: 2 };
        assert_eq!(c.period_days(), 14);
        assert_eq!(c.next_on_or_after(Date::parse("2026-08-10").unwrap()).iso(), "2026-08-17");
        assert_eq!(c.latest_on_or_before(Date::parse("2026-08-16").unwrap()).iso(), "2026-08-03");
    }

    #[test]
    fn describe_reads_the_weekday_off_the_anchor() {
        assert_eq!(weekly().describe(), "every Monday");
        assert_eq!(
            Cadence { anchor: Date::parse("2026-08-03").unwrap(), every_n_weeks: 2 }.describe(),
            "every 2 weeks on Monday"
        );
    }
}
