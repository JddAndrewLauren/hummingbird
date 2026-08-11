//! "Is a race starting inside the lead time?" — the whole of what
//! `race-alert-poll` decides, as a pure function of (stored season, now).
//!
//! This module is the reason the lane has two crons rather than one: nothing
//! here needs the network, so asking it every fifteen minutes costs nothing
//! upstream while the schedule itself is refreshed four times a day. See
//! `lib.rs` for the full argument.

use crate::body::RaceEvent;

/// How far ahead of a race start the alert fires.
///
/// A named `const` in the lib, like `ALARM_INTERVAL_MS` — **not** a binding
/// and not an env var. "Configured" here means *named and documented*, not
/// *user-tunable*: `client/core/src/bindings.rs`'s key vocabulary is closed,
/// kebab-case and unversioned, and adding a fourth key would need a client
/// change for a number nobody will tune.
pub const LEAD_MS: i64 = 90 * 60 * 1000;

/// The race to raise about right now, if any: the earliest race whose start
/// is still ahead and no more than `lead_ms` away.
///
/// A race already under way is **not** returned — the alert is a
/// get-to-the-couch nudge, and one that arrives after lights out is worse
/// than none. Everything in the past is simply skipped, which is what makes
/// an off-season (every race behind us) answer `None` rather than fail.
pub fn race_within_lead(events: &[RaceEvent], now_ms: i64, lead_ms: i64) -> Option<&RaceEvent> {
    events
        .iter()
        .filter(|event| {
            let until = event.starts_at_ms - now_ms;
            (0..=lead_ms).contains(&until)
        })
        // Feed order is chronological, but the answer must not depend on
        // that: two races inside one lead window would be a schedule this
        // adapter has never seen, and picking the *earlier* is the only
        // reading that makes sense of "starts soon".
        .min_by_key(|event| event.starts_at_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RACE: i64 = 1_772_942_400_000;

    fn race_at(name: &str, starts_at_ms: i64) -> RaceEvent {
        RaceEvent {
            name: name.to_string(),
            locality: "Melbourne".to_string(),
            starts_at_ms,
            sessions: vec![],
        }
    }

    fn season() -> Vec<RaceEvent> {
        vec![
            race_at("Australian Grand Prix", RACE),
            race_at("Chinese Grand Prix", RACE + 14 * 24 * 60 * 60 * 1000),
        ]
    }

    /// The tracer: a race exactly one lead away is the answer.
    #[test]
    fn a_race_at_the_lead_time_is_the_one_to_raise_about() {
        let season = season();
        let found = race_within_lead(&season, RACE - LEAD_MS, LEAD_MS);
        assert_eq!(found.map(|e| e.name.as_str()), Some("Australian Grand Prix"));
    }

    /// The window is closed at both ends, and the cron's own ±15 minutes
    /// lands inside it — six polls of one race weekend all answer the same
    /// race, which is what lets the binary hold no state.
    #[test]
    fn every_poll_inside_the_window_answers_the_same_race() {
        let season = season();
        let quarter_hour = 15 * 60 * 1000;
        let mut answered = 0;
        let mut now = RACE - LEAD_MS;
        while now <= RACE {
            let found = race_within_lead(&season, now, LEAD_MS)
                .unwrap_or_else(|| panic!("nothing at {now}"));
            assert_eq!(found.starts_at_ms, RACE);
            answered += 1;
            now += quarter_hour;
        }
        assert_eq!(answered, 7, "the whole 90-minute window, at the alert cron");
    }

    /// Outside the lead, nothing — a season is mostly this.
    #[test]
    fn a_race_further_out_than_the_lead_says_nothing_yet() {
        let season = season();
        assert!(race_within_lead(&season, RACE - LEAD_MS - 1, LEAD_MS).is_none());
    }

    /// A race already under way is not raised about. This is the arm that
    /// separates "starts soon" from "started" — an alert that arrives after
    /// lights out is worse than none.
    #[test]
    fn a_race_already_under_way_is_not_raised_about() {
        let season = season();
        assert!(race_within_lead(&season, RACE + 1, LEAD_MS).is_none());
        assert!(race_within_lead(&season, RACE + 2 * 60 * 60 * 1000, LEAD_MS).is_none());
    }

    /// **Off-season**: the season is stored and every race in it is behind
    /// us. That is silence, not a failure — the same discrimination the
    /// snapshot side draws (`lib.rs`'s outcome 3).
    #[test]
    fn an_off_season_answers_nothing_rather_than_failing() {
        let season = season();
        let a_year_later = RACE + 365 * 24 * 60 * 60 * 1000;
        assert!(race_within_lead(&season, a_year_later, LEAD_MS).is_none());
    }

    /// An empty season — a body that parsed and carries no events — is the
    /// same silence, reached from the other side.
    #[test]
    fn an_empty_season_answers_nothing() {
        assert!(race_within_lead(&[], RACE - LEAD_MS, LEAD_MS).is_none());
    }

    /// Two races inside one window is a schedule this adapter has never
    /// seen; the earlier one is the only reading of "starts soon" that makes
    /// sense, and it must not depend on feed order.
    #[test]
    fn two_races_inside_one_window_answer_with_the_earlier() {
        let crowded = vec![
            race_at("Later", RACE + 30 * 60 * 1000),
            race_at("Earlier", RACE),
        ];
        let found = race_within_lead(&crowded, RACE - LEAD_MS, LEAD_MS);
        assert_eq!(found.map(|e| e.name.as_str()), Some("Earlier"));
    }
}
