//! Whether this `*/15` run is worth a Yahoo fetch — a pure function of
//! (what is stored, now).
//!
//! The binary reads the stored rows back (one cheap authority `GET` each, as
//! `race-alert-poll` already does) and fetches **only** when one of these
//! holds, checked in this order:
//!
//! 1. nothing has ever been fetched;
//! 2. the last fetch is **over** [`STALE_AFTER_MS`] (6h) old — the floor
//!    that makes [`crate::body::POLLED_EVERY_MS`] a true declaration;
//! 3. a stored lineup is invalid and its deadline is within
//!    [`LOCK_WINDOW_MS`] (24h) — crunch time, when a fix made in Yahoo must
//!    be seen within one run, so a ring is never escalated for a lineup
//!    already fixed;
//! 4. the waiver deadline is within [`WAIVER_WINDOW_MS`] (12h) from now.
//!
//! and **never** when the Fantasy question is switched off (ADR-0034:
//! off means unpolled).
//!
//! **A lineup deadline that has already passed still counts as within the
//! window.** A stored deadline is decided against its own fetch instant,
//! and [`crate::deadline`] never names a kickoff that had already happened
//! — so a passed one only means the row is behind (the replacement it
//! named is locked now, and a later one may still be in play). One fetch
//! moves it forward or clears it, so this cannot loop. A waiver deadline
//! gets no such reading: its body is #719's, and this lane cannot promise
//! that a fresh fetch never reports a closed one.

use crate::body::{LineupBody, POLLED_EVERY_MS};

/// The last fetch is stale once it is **over** this old.
pub const STALE_AFTER_MS: i64 = POLLED_EVERY_MS;

/// An invalid lineup is in crunch time once its deadline is this close.
pub const LOCK_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;

/// The waiver deadline is in crunch time once it is this close.
pub const WAIVER_WINDOW_MS: i64 = 12 * 60 * 60 * 1000;

/// What the gate reads: the stored rows, already parsed.
#[derive(Debug, Clone, Default)]
pub struct Stored {
    /// When Yahoo was last fetched: the discovered-leagues row's
    /// `fetched_at`, which every successful fetch writes whatever the
    /// binding says. `None` when there is no such row.
    pub last_fetch_ms: Option<i64>,
    /// Every stored lineup this run could read back.
    pub lineups: Vec<LineupBody>,
    /// The current waiver deadline, once the waivers subject stores one
    /// (#719); `None` until then.
    pub waiver_deadline_ms: Option<i64>,
}

/// Why a run fetches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchReason {
    NeverFetched,
    Stale,
    /// The team whose invalid lineup locks within the window.
    LineupNearLock { team_key: String },
    WaiverNear,
}

/// Why a run does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    SwitchedOff,
    Fresh,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Fetch(FetchReason),
    Skip(SkipReason),
}

/// The gate. `enabled` is [`crate::enabled::enabled_from_response`]'s answer.
pub fn decide(enabled: bool, stored: &Stored, now_ms: i64) -> Decision {
    if !enabled {
        return Decision::Skip(SkipReason::SwitchedOff);
    }
    let Some(last_fetch_ms) = stored.last_fetch_ms else {
        return Decision::Fetch(FetchReason::NeverFetched);
    };
    if now_ms - last_fetch_ms > STALE_AFTER_MS {
        return Decision::Fetch(FetchReason::Stale);
    }
    for lineup in &stored.lineups {
        if let Some(deadline_ms) = lineup.deadline_ms() {
            if deadline_ms - now_ms <= LOCK_WINDOW_MS {
                return Decision::Fetch(FetchReason::LineupNearLock { team_key: lineup.team_key.clone() });
            }
        }
    }
    if let Some(deadline_ms) = stored.waiver_deadline_ms {
        if (0..=WAIVER_WINDOW_MS).contains(&(deadline_ms - now_ms)) {
            return Decision::Fetch(FetchReason::WaiverNear);
        }
    }
    Decision::Skip(SkipReason::Fresh)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 60 * 60 * 1000;
    const ROSTER: &str = include_str!("../tests/fixtures/roster.json");
    /// The fixture's fetch instant; its invalid lineup's deadline is
    /// Thursday night, 57h15m later.
    const AS_OF_MS: i64 = 1_790_694_000_000;
    const DEADLINE_MS: i64 = 1_790_900_100_000;

    fn invalid() -> LineupBody {
        LineupBody::from_roster(&serde_json::from_str(ROSTER).unwrap(), AS_OF_MS)
    }

    fn valid() -> LineupBody {
        let mut lineup = invalid();
        lineup.slots.iter_mut().for_each(|slot| slot.invalid = None);
        lineup
    }

    fn stored(last_fetch_ms: i64, lineups: Vec<LineupBody>) -> Stored {
        Stored { last_fetch_ms: Some(last_fetch_ms), lineups, waiver_deadline_ms: None }
    }

    #[test]
    fn fresh_and_valid_does_not_fetch() {
        let now = AS_OF_MS + HOUR;
        assert_eq!(decide(true, &stored(AS_OF_MS, vec![valid()]), now), Decision::Skip(SkipReason::Fresh));
    }

    #[test]
    fn invalid_within_24h_of_its_lock_fetches() {
        let now = DEADLINE_MS - 23 * HOUR;
        let decision = decide(true, &stored(now - HOUR, vec![invalid()]), now);
        assert_eq!(
            decision,
            Decision::Fetch(FetchReason::LineupNearLock { team_key: "449.l.123456.t.7".to_string() })
        );
    }

    /// Invalid, but the lock is a day and a half away: the six-hour floor is
    /// enough, and Yahoo is not asked every fifteen minutes.
    #[test]
    fn invalid_but_far_from_its_lock_does_not_fetch() {
        let now = DEADLINE_MS - 25 * HOUR;
        assert_eq!(decide(true, &stored(now - HOUR, vec![invalid()]), now), Decision::Skip(SkipReason::Fresh));
    }

    #[test]
    fn a_lineup_deadline_that_passed_since_the_fetch_fetches() {
        let now = DEADLINE_MS + HOUR;
        assert!(matches!(
            decide(true, &stored(now - HOUR, vec![invalid()]), now),
            Decision::Fetch(FetchReason::LineupNearLock { .. })
        ));
    }

    #[test]
    fn a_waiver_deadline_within_12h_fetches() {
        let now = AS_OF_MS + HOUR;
        let mut stored = stored(AS_OF_MS, vec![valid()]);
        stored.waiver_deadline_ms = Some(now + 11 * HOUR);
        assert_eq!(decide(true, &stored, now), Decision::Fetch(FetchReason::WaiverNear));
        stored.waiver_deadline_ms = Some(now + 13 * HOUR);
        assert_eq!(decide(true, &stored, now), Decision::Skip(SkipReason::Fresh));
        stored.waiver_deadline_ms = Some(now - HOUR);
        assert_eq!(decide(true, &stored, now), Decision::Skip(SkipReason::Fresh), "a closed window");
    }

    #[test]
    fn a_last_fetch_over_6h_old_fetches() {
        let now = AS_OF_MS + 6 * HOUR + 1;
        assert_eq!(decide(true, &stored(AS_OF_MS, vec![valid()]), now), Decision::Fetch(FetchReason::Stale));
        let now = AS_OF_MS + 6 * HOUR;
        assert_eq!(
            decide(true, &stored(AS_OF_MS, vec![valid()]), now),
            Decision::Skip(SkipReason::Fresh),
            "exactly six hours is not over six hours"
        );
    }

    #[test]
    fn nothing_ever_fetched_fetches() {
        assert_eq!(decide(true, &Stored::default(), AS_OF_MS), Decision::Fetch(FetchReason::NeverFetched));
    }

    /// ADR-0034: off means unpolled — whatever else the stored rows say.
    #[test]
    fn a_question_switched_off_never_fetches() {
        let now = DEADLINE_MS - HOUR;
        let mut crunch = stored(AS_OF_MS, vec![invalid()]);
        crunch.waiver_deadline_ms = Some(now + HOUR);
        assert_eq!(decide(false, &crunch, now), Decision::Skip(SkipReason::SwitchedOff));
        assert_eq!(decide(false, &Stored::default(), now), Decision::Skip(SkipReason::SwitchedOff));
    }
}
