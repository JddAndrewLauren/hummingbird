//! The waking-window clamp (ADR-0035): when a ring the poller times itself
//! may fire, in the operator's civil time.
//!
//! A ring is due at `deadline − lead`. If that instant falls outside
//! [`WAKING_START`]..=[`WAKING_END`] it moves **backwards** to the end of
//! the previous window, and **never forwards**: a ring after the deadline is
//! worse than none. A ring with no waking moment in its lead range is
//! **not delivered at all** ([`ring_at`] returns `None`). For a 06:30 lock
//! the escalation, due at 05:00, has nowhere to go, so the lock gets one
//! ring at 22:00 the night before. That is accuracy, not degradation.
//!
//! **The lead range.** A ring whose lead is at least [`MIN_LEAD`] is the
//! first warning, and may move back as far as the previous window's end, so
//! it always rings. A shorter lead is an escalation of that warning. It may
//! move back only while it stays strictly after `deadline − MIN_LEAD`, so it
//! can never land on or before the first ring.
//!
//! **Civil time comes from the zone, never from a fixed offset.** "22:00
//! the previous evening" is resolved in the zone on that evening, so a
//! Sunday lock on the day US clocks go back still rings at 22:00 Saturday
//! by the wall clock (NFL Week 9's case).
//!
//! The constants are named here, not in `settings`, for ADR-0016 Q3's
//! reason: `settings` is device-writable with no DELETE, and no reader
//! benefits from tuning them. If 22:00 is past the operator's bedtime, the
//! fix is editing [`WAKING_END`] and redeploying.
//!
//! **This module is the crate's tzdb.** `tests/tzdb_fence.rs` proves the
//! tz database stays out of `domain`, `hummingbird-core`, every `ffi-*`
//! crate and the authority worker. The panes' own civil-date reasoning
//! still crosses ADR-0025's zone bridge. Don't move this into a shared
//! crate before its second user (#721) argues for one.

use jiff::civil::{time, Time};
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};

/// The first waking moment of a day, inclusive.
pub const WAKING_START: Time = time(8, 0, 0, 0);
/// The last waking moment of a day, inclusive: where an early ring moves to.
pub const WAKING_END: Time = time(22, 0, 0, 0);
/// The lead of the first warning: a ring is due this long before its
/// deadline.
pub const MIN_LEAD: SignedDuration = SignedDuration::from_hours(3);

/// When a ring warning `lead` ahead of `deadline` fires in `zone`, or
/// `None` when its lead range holds no waking moment. The result is never
/// later than `deadline − lead`, whatever the inputs, including a deadline
/// already past: the clamp reads no clock.
pub fn ring_at(deadline: Timestamp, lead: SignedDuration, zone: &TimeZone) -> Option<Timestamp> {
    let due = deadline.checked_sub(lead).ok()?;
    let ring = latest_waking_at_or_before(due, zone)?;
    if lead < MIN_LEAD {
        let first_ring_due = deadline.checked_sub(MIN_LEAD).ok()?;
        if ring <= first_ring_due {
            return None;
        }
    }
    Some(ring)
}

/// The latest instant at or before `due` whose civil time in `zone` lies
/// inside the waking window.
fn latest_waking_at_or_before(due: Timestamp, zone: &TimeZone) -> Option<Timestamp> {
    let local = due.to_zoned(zone.clone());
    let clock = local.time();
    if (WAKING_START..=WAKING_END).contains(&clock) {
        return Some(due);
    }
    // Before the window opens, the window to fall back to is yesterday's.
    let day = if clock < WAKING_START { local.date().yesterday().ok()? } else { local.date() };
    let end = day.to_datetime(WAKING_END).to_zoned(zone.clone()).ok()?.timestamp();
    // 22:00 is never inside a transition in any zone this serves, but a
    // zone whose evening it were would get no ring rather than a late one.
    (end <= due).then_some(end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::{date, DateTime};

    /// #857's escalation lead, used here only to exercise the clamp.
    const ESCALATION_LEAD: SignedDuration = SignedDuration::from_mins(90);

    fn pacific() -> TimeZone {
        TimeZone::get("America/Los_Angeles").expect("the tzdb knows Pacific time")
    }

    /// A Pacific wall-clock reading as an instant.
    fn pt(at: DateTime) -> Timestamp {
        at.to_zoned(pacific()).expect("an unambiguous Pacific time").timestamp()
    }

    fn first_ring(lock: DateTime) -> Option<Timestamp> {
        ring_at(pt(lock), MIN_LEAD, &pacific())
    }

    /// Every ring a lock gets, first warning then escalation.
    fn rings(lock: DateTime) -> Vec<Timestamp> {
        [MIN_LEAD, ESCALATION_LEAD]
            .into_iter()
            .filter_map(|lead| ring_at(pt(lock), lead, &pacific()))
            .collect()
    }

    // ADR-0035's table, row by row, in week 4 of 2026 (Thu 1 Oct to Mon
    // 5 Oct, Pacific daylight time).

    #[test]
    fn a_sunday_1pm_lock_rings_at_10am() {
        assert_eq!(first_ring(date(2026, 10, 4).at(13, 0, 0, 0)), Some(pt(date(2026, 10, 4).at(10, 0, 0, 0))));
    }

    #[test]
    fn a_thursday_night_lock_rings_at_2_15pm() {
        assert_eq!(first_ring(date(2026, 10, 1).at(17, 15, 0, 0)), Some(pt(date(2026, 10, 1).at(14, 15, 0, 0))));
    }

    #[test]
    fn a_london_lock_at_6_30am_rings_at_22_00_the_previous_day() {
        assert_eq!(first_ring(date(2026, 10, 4).at(6, 30, 0, 0)), Some(pt(date(2026, 10, 3).at(22, 0, 0, 0))));
    }

    #[test]
    fn a_munich_lock_at_5_30am_rings_at_22_00_the_previous_day() {
        assert_eq!(first_ring(date(2026, 10, 4).at(5, 30, 0, 0)), Some(pt(date(2026, 10, 3).at(22, 0, 0, 0))));
    }

    #[test]
    fn a_monday_night_lock_rings_at_2_15pm() {
        assert_eq!(first_ring(date(2026, 10, 5).at(17, 15, 0, 0)), Some(pt(date(2026, 10, 5).at(14, 15, 0, 0))));
    }

    /// The case nobody raised (ADR-0035): a waiver deadline at 09:00
    /// Eastern is 06:00 Pacific, and rings at 22:00 the night before.
    #[test]
    fn a_6am_waiver_deadline_rings_at_22_00_the_night_before() {
        assert_eq!(first_ring(date(2026, 10, 7).at(6, 0, 0, 0)), Some(pt(date(2026, 10, 6).at(22, 0, 0, 0))));
    }

    // The window's two edges. Both bounds are inclusive: 22:00 is where an
    // early ring moves to, so it is a waking moment itself.

    #[test]
    fn a_deadline_at_exactly_08_00_rings_at_22_00_the_previous_day() {
        assert_eq!(first_ring(date(2026, 10, 4).at(8, 0, 0, 0)), Some(pt(date(2026, 10, 3).at(22, 0, 0, 0))));
    }

    #[test]
    fn a_deadline_at_exactly_22_00_rings_at_19_00() {
        assert_eq!(first_ring(date(2026, 10, 4).at(22, 0, 0, 0)), Some(pt(date(2026, 10, 4).at(19, 0, 0, 0))));
    }

    #[test]
    fn a_ring_due_at_exactly_08_00_or_22_00_is_not_moved() {
        assert_eq!(first_ring(date(2026, 10, 4).at(11, 0, 0, 0)), Some(pt(date(2026, 10, 4).at(8, 0, 0, 0))));
        assert_eq!(first_ring(date(2026, 10, 5).at(1, 0, 0, 0)), Some(pt(date(2026, 10, 4).at(22, 0, 0, 0))));
    }

    #[test]
    fn a_ring_due_late_in_the_evening_moves_back_to_22_00_that_evening() {
        assert_eq!(first_ring(date(2026, 10, 5).at(1, 30, 0, 0)), Some(pt(date(2026, 10, 4).at(22, 0, 0, 0))));
    }

    /// The acceptance box. A London lock's escalation is due at 05:00, and
    /// there is no waking moment between the first ring's due time and it,
    /// so the lock rings once. Asserted as an absence: the escalation
    /// returns nothing, and the lock's ring list holds exactly one ring.
    #[test]
    fn a_lock_whose_escalation_falls_outside_the_window_rings_once_not_twice() {
        let london = date(2026, 10, 4).at(6, 30, 0, 0);
        assert_eq!(ring_at(pt(london), ESCALATION_LEAD, &pacific()), None, "no escalation ring");
        assert_eq!(rings(london), vec![pt(date(2026, 10, 3).at(22, 0, 0, 0))]);
    }

    #[test]
    fn a_lock_whose_escalation_falls_inside_the_window_rings_twice() {
        let sunday = date(2026, 10, 4).at(13, 0, 0, 0);
        assert_eq!(rings(sunday), vec![pt(date(2026, 10, 4).at(10, 0, 0, 0)), pt(date(2026, 10, 4).at(11, 30, 0, 0))]);
    }

    // The two daylight-saving changeover Sundays of 2026. The ring is
    // resolved in the zone on the evening it lands, so it reads 22:00 by
    // the wall clock either side of the change, not 22:00 ± 1h.

    /// Clocks go back at 02:00 on Sun 1 Nov 2026. A 06:30 PST lock rings at
    /// 22:00 PDT on Saturday, which is 05:00Z. Carrying Sunday's −8 offset
    /// back to Saturday would ring at 23:00 PDT.
    #[test]
    fn a_lock_on_the_sunday_clocks_go_back_rings_at_22_00_saturday_wall_clock() {
        let ring = first_ring(date(2026, 11, 1).at(6, 30, 0, 0)).expect("a first ring");
        assert_eq!(ring, "2026-11-01T05:00:00Z".parse::<Timestamp>().unwrap());
        assert_eq!(ring.to_zoned(pacific()).datetime(), date(2026, 10, 31).at(22, 0, 0, 0));
    }

    /// A daytime lock that day keeps its plain three-hour lead: 13:00 PST
    /// rings at 10:00 PST, 18:00Z.
    #[test]
    fn a_daytime_lock_on_the_sunday_clocks_go_back_rings_three_elapsed_hours_before() {
        let ring = first_ring(date(2026, 11, 1).at(13, 0, 0, 0)).expect("a first ring");
        assert_eq!(ring, "2026-11-01T18:00:00Z".parse::<Timestamp>().unwrap());
        assert_eq!(ring.to_zoned(pacific()).datetime(), date(2026, 11, 1).at(10, 0, 0, 0));
    }

    /// Clocks go forward at 02:00 on Sun 8 Mar 2026. A 06:30 PDT lock
    /// rings at 22:00 PST on Saturday, which is 06:00Z. Carrying Sunday's
    /// −7 offset back would ring at 21:00 PST.
    #[test]
    fn a_lock_on_the_sunday_clocks_go_forward_rings_at_22_00_saturday_wall_clock() {
        let ring = first_ring(date(2026, 3, 8).at(6, 30, 0, 0)).expect("a first ring");
        assert_eq!(ring, "2026-03-08T06:00:00Z".parse::<Timestamp>().unwrap());
        assert_eq!(ring.to_zoned(pacific()).datetime(), date(2026, 3, 7).at(22, 0, 0, 0));
    }

    #[test]
    fn a_daytime_lock_on_the_sunday_clocks_go_forward_rings_three_elapsed_hours_before() {
        let ring = first_ring(date(2026, 3, 8).at(13, 0, 0, 0)).expect("a first ring");
        assert_eq!(ring, "2026-03-08T17:00:00Z".parse::<Timestamp>().unwrap());
        assert_eq!(ring.to_zoned(pacific()).datetime(), date(2026, 3, 8).at(10, 0, 0, 0));
    }

    /// The clamp reads no clock, so a deadline years past clamps exactly as
    /// it did then.
    #[test]
    fn a_deadline_already_past_clamps_without_reference_to_now() {
        let lock = date(2020, 10, 4).at(6, 30, 0, 0);
        assert_eq!(first_ring(lock), Some(pt(date(2020, 10, 3).at(22, 0, 0, 0))));
    }

    /// The property: under any deadline, lead and zone, a ring is never
    /// later than `deadline − lead`, and when there is one its wall-clock
    /// time is inside the window. Swept every 13 minutes across 2025 and
    /// 2026 (both changeover Sundays of each year, deadlines long past) for
    /// leads from zero to twelve hours, in zones either side of the date
    /// line and one with no daylight saving.
    #[test]
    fn a_ring_is_never_moved_forwards_and_always_lands_in_the_window() {
        let zones = ["America/Los_Angeles", "Europe/London", "Australia/Sydney", "Asia/Kolkata"]
            .map(|name| TimeZone::get(name).expect("the tzdb knows the zone"));
        let leads = [0, 1, 89, 90, 91, 179, 180, 181, 600, 720].map(SignedDuration::from_mins);
        let start = "2025-01-01T00:00:00Z".parse::<Timestamp>().unwrap();
        let end = "2027-01-01T00:00:00Z".parse::<Timestamp>().unwrap();
        let step = SignedDuration::from_mins(13);
        let mut checked = 0;
        for zone in &zones {
            let mut deadline = start;
            while deadline < end {
                for lead in leads {
                    let due = deadline - lead;
                    if let Some(ring) = ring_at(deadline, lead, zone) {
                        assert!(ring <= due, "{deadline} lead {lead:?}: ring {ring} after due {due}");
                        let clock = ring.to_zoned(zone.clone()).time();
                        assert!(
                            (WAKING_START..=WAKING_END).contains(&clock),
                            "{deadline} lead {lead:?}: ring {ring} at {clock}"
                        );
                    } else {
                        assert!(lead < MIN_LEAD, "{deadline} lead {lead:?}: a first warning always rings");
                    }
                    checked += 1;
                }
                deadline += step;
            }
        }
        assert!(checked > 1_000_000, "the sweep covered {checked} cases");
    }
}
