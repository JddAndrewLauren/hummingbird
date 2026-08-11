//! The Jolpica response, read against a saved verbatim sample of the real
//! thing (`tests/fixtures/jolpica-current.json`).
//!
//! # The feed
//!
//! **[Jolpica](https://github.com/jolpica/jolpica-f1)**, the maintained
//! successor to the retired Ergast API:
//! `GET https://api.jolpi.ca/ergast/f1/current.json` — the whole season in
//! one ~14 KB call, every race and every session carrying a UTC `date` +
//! `time`, in two ladder shapes (conventional FP1/FP2/FP3/Qualifying, and
//! sprint FP1/SprintQualifying/Sprint/Qualifying). Both shapes are in the
//! one committed fixture.
//!
//! Two facts belong here rather than in a commit message, per ADR-0009's
//! "verified at wiring time and documented in the adapter" caveat:
//!
//! * **A custom `User-Agent` is load-bearing, not politeness.** An unset or
//!   default UA is answered `403`, measured. That is exactly the class of
//!   fact a later tidy-up of the HTTP client deletes, so it is set in
//!   `race_schedule_poll.rs` with the same warning on the header itself.
//! * **Jolpica is a volunteer-run free service with no SLA**, 4 req/s burst
//!   and 500 req/hr unauthenticated, and its own docs say the limits may
//!   tighten. The two-cron split (see `lib.rs`) is what keeps this lane at 4
//!   requests a day rather than 96.
//!
//! `current.json` is used deliberately over `2026.json` — no hardcoded year
//! and no year-rollover arithmetic. `current/next.json` is **rejected**:
//! storing "the next race" would be storing an answer, which ADR-0002
//! forbids. The snapshot holds the season; "next" is computed at read time
//! ([`crate::next`], and the pane).
//!
//! # What is required, and what is merely read
//!
//! The race's own `raceName`, `date`, `time` and `Circuit.Location.locality`
//! are **required**, and an absent one is a named [`ScheduleError`] that
//! writes nothing — `city-waste`'s `PageError::Missing` rule, so a feed
//! redesign fails loudly on the first poll instead of writing something
//! plausible. A supporting session is optional by construction (IndyCar,
//! when it lands, has a race start and no ladder at all), so an absent one
//! is silence — but a session that is *present and malformed* is the same
//! loud error, since that is a shape change rather than an absence.
//!
//! A session key this build does not recognise is ignored rather than
//! rejected: the ladder is additive, and the fields the answer actually
//! depends on — the race start, the name, the place — are the required ones
//! above.
//!
//! **No tzdb.** Every stamp in this feed is UTC (`"04:00:00Z"`), and a race
//! start is an instant, so [`instant_ms`] is dependency-free integer
//! arithmetic and a non-`Z` stamp is refused rather than guessed at.

use serde_json::Value;

use crate::body::{RaceEvent, RaceScheduleBody, Session};

/// The one endpoint this adapter reads. `current`, never a year.
pub const FEED_URL: &str = "https://api.jolpi.ca/ergast/f1/current.json";

/// The supporting ladder, in the order a weekend runs it. Declaration order
/// is chronological for **both** shapes the feed publishes — conventional
/// (FP1, FP2, FP3, Qualifying) and sprint (FP1, Sprint Qualifying, Sprint,
/// Qualifying) — which is why the body needs no sort and no clock to build.
/// `tests::sessions_are_in_chronological_order_for_both_ladder_shapes` pins
/// that against the real fixture rather than trusting the claim.
const LADDER: &[(&str, &str, &str)] = &[
    ("FirstPractice", "practice", "Practice 1"),
    ("SecondPractice", "practice", "Practice 2"),
    ("ThirdPractice", "practice", "Practice 3"),
    ("SprintQualifying", "sprint_qualifying", "Sprint Qualifying"),
    ("Sprint", "sprint", "Sprint"),
    ("Qualifying", "qualifying", "Qualifying"),
];

/// Why a 200 could not be read as a season. Every arm names *what* was
/// wrong and *where*, so a feed redesign is one legible log line rather than
/// a body that parsed into something plausible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleError {
    NotJson(String),
    /// The response parsed but has no `MRData.RaceTable.Races` array at all.
    NotARaceTable,
    /// The table is there and empty. **Not** read as an off-season: the feed
    /// answers `current` with the whole season all year, so zero races means
    /// the feed's shape or its notion of "current" moved, not that nothing
    /// is scheduled. Off-season is a season whose races are all in the past,
    /// and that is a legitimate body (see `lib.rs`'s outcome 3).
    NoRaces,
    /// A required field is absent from one race, named with the round it
    /// came from.
    Missing { round: String, field: &'static str },
    /// A date/time pair is present but not a UTC instant this build will
    /// guess at.
    BadInstant { round: String, field: &'static str, value: String },
}

impl std::fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScheduleError::NotJson(reason) => write!(f, "the feed's answer is not JSON: {reason}"),
            ScheduleError::NotARaceTable => {
                write!(f, "the feed's answer carries no MRData.RaceTable.Races")
            }
            ScheduleError::NoRaces => write!(f, "the feed's race table is empty"),
            ScheduleError::Missing { round, field } => {
                write!(f, "round {round} has no `{field}`")
            }
            ScheduleError::BadInstant { round, field, value } => {
                write!(f, "round {round}'s `{field}` is not a UTC instant: {value}")
            }
        }
    }
}

/// Turns one `date` + `time` pair from the feed into epoch milliseconds.
///
/// Both are UTC by the feed's own contract (`"2026-03-08"` + `"04:00:00Z"`),
/// so this is pure integer arithmetic — the days-from-civil algorithm, and
/// no time zone to resolve. A stamp without the trailing `Z`, or with parts
/// that are not numbers, is `None`: reading a local-looking time as UTC
/// would move a race start by hours, silently.
pub fn instant_ms(date: &str, time: &str) -> Option<i64> {
    let (year, rest) = date.split_once('-')?;
    let (month, day) = rest.split_once('-')?;
    let (year, month, day): (i64, i64, i64) =
        (year.parse().ok()?, month.parse().ok()?, day.parse().ok()?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let time = time.strip_suffix('Z')?;
    let mut parts = time.split(':');
    let hour: i64 = parts.next()?.parse().ok()?;
    let minute: i64 = parts.next()?.parse().ok()?;
    // Seconds are optional in the feed's own shapes; a fractional second is
    // truncated rather than refused.
    let second: i64 = match parts.next() {
        None => 0,
        Some(s) => s.split('.').next()?.parse().ok()?,
    };
    if parts.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    Some((days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second) * 1_000)
}

/// Days since 1970-01-01 for a proleptic Gregorian civil date (Howard
/// Hinnant's `days_from_civil`, the same algorithm `city-waste`'s `date.rs`
/// carries — copied rather than shared because that crate is a poller, not a
/// library, and neither may depend on the other).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Reads one Jolpica `current.json` response into the body this poller
/// writes. Whole season, unfiltered, in feed order, and no clock anywhere.
pub fn parse(response: &str) -> Result<RaceScheduleBody, ScheduleError> {
    let value: Value =
        serde_json::from_str(response).map_err(|e| ScheduleError::NotJson(e.to_string()))?;
    let races = value
        .pointer("/MRData/RaceTable/Races")
        .and_then(Value::as_array)
        .ok_or(ScheduleError::NotARaceTable)?;
    if races.is_empty() {
        return Err(ScheduleError::NoRaces);
    }

    let mut events = Vec::with_capacity(races.len());
    for race in races {
        // The round is provenance for the error messages only — it is
        // deliberately not carried into the body, since the occurrence key
        // is the start instant and nothing downstream reads a round.
        let round = race
            .get("round")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string();
        let name = text(race, "raceName", &round)?;
        let locality = race
            .pointer("/Circuit/Location/locality")
            .and_then(Value::as_str)
            .ok_or(ScheduleError::Missing { round: round.clone(), field: "Circuit.Location.locality" })?
            .to_string();
        let starts_at_ms = stamp(race, "date", "time", "the race start", &round)?;

        let mut sessions = Vec::new();
        for (feed_key, kind, label) in LADDER {
            let Some(session) = race.get(feed_key) else {
                continue;
            };
            sessions.push(Session {
                kind: (*kind).to_string(),
                label: (*label).to_string(),
                starts_at_ms: stamp(session, "date", "time", feed_key, &round)?,
            });
        }

        events.push(RaceEvent { name, locality, starts_at_ms, sessions });
    }
    Ok(RaceScheduleBody { events })
}

fn text(value: &Value, field: &'static str, round: &str) -> Result<String, ScheduleError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or(ScheduleError::Missing { round: round.to_string(), field })
}

fn stamp(
    value: &Value,
    date_field: &'static str,
    time_field: &'static str,
    what: &'static str,
    round: &str,
) -> Result<i64, ScheduleError> {
    let date = text(value, date_field, round).map_err(|_| ScheduleError::Missing {
        round: round.to_string(),
        field: what,
    })?;
    let time = text(value, time_field, round).map_err(|_| ScheduleError::Missing {
        round: round.to_string(),
        field: what,
    })?;
    instant_ms(&date, &time).ok_or(ScheduleError::BadInstant {
        round: round.to_string(),
        field: what,
        value: format!("{date} {time}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real response, verbatim — see `tests/fixtures/README.md` for why
    /// it is NOT reduced or sanitised the way `city-waste`'s fixtures are.
    const FIXTURE: &str = include_str!("../tests/fixtures/jolpica-current.json");

    fn season() -> RaceScheduleBody {
        parse(FIXTURE).expect("the saved response parses")
    }

    /// The tracer: the real feed's first race, whole.
    #[test]
    fn the_saved_response_parses_the_whole_season() {
        let season = season();
        assert_eq!(season.events.len(), 23, "the whole season, unfiltered");
        let first = &season.events[0];
        assert_eq!(first.name, "Australian Grand Prix");
        assert_eq!(first.locality, "Melbourne");
        // 2026-03-08T04:00:00Z, the race start — not the first session's.
        assert_eq!(first.starts_at_ms, 1_772_942_400_000);
        assert!(
            first.sessions.iter().all(|s| s.starts_at_ms < first.starts_at_ms),
            "`sessions` holds only the supporting ladder, never the race"
        );
    }

    /// **Both ladder shapes are already in the one fixture**, which is what
    /// makes the sprint weekend a real case rather than a hand-built one.
    #[test]
    fn both_ladder_shapes_are_read_from_the_one_fixture() {
        let season = season();
        let conventional: Vec<Vec<&str>> = season
            .events
            .iter()
            .map(|e| e.sessions.iter().map(|s| s.label.as_str()).collect())
            .collect();
        let shapes: std::collections::BTreeSet<Vec<&str>> =
            conventional.into_iter().collect();
        assert_eq!(
            shapes,
            [
                vec!["Practice 1", "Practice 2", "Practice 3", "Qualifying"],
                vec!["Practice 1", "Sprint Qualifying", "Sprint", "Qualifying"],
            ]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
            "the feed publishes exactly two ladder shapes and both are read"
        );
    }

    /// The claim `LADDER`'s declaration order rests on, checked rather than
    /// asserted in a comment: for every race of either shape, the ladder as
    /// built runs forwards in time and finishes before the race starts.
    #[test]
    fn sessions_are_in_chronological_order_for_both_ladder_shapes() {
        for event in season().events {
            let stamps: Vec<i64> = event.sessions.iter().map(|s| s.starts_at_ms).collect();
            let mut sorted = stamps.clone();
            sorted.sort_unstable();
            assert_eq!(stamps, sorted, "{} ladder is out of order", event.name);
            assert!(
                stamps.last().is_some_and(|last| *last < event.starts_at_ms),
                "{}'s ladder must finish before the race",
                event.name
            );
        }
    }

    /// A sprint weekend's session *kinds* are distinguishable from a
    /// practice's — three practices share one kind, so the pane needs both
    /// the kind and the label.
    #[test]
    fn a_sprint_weekend_carries_its_own_session_kinds() {
        let sprint = season()
            .events
            .iter()
            .find(|e| e.sessions.iter().any(|s| s.kind == "sprint"))
            .expect("the season has a sprint weekend")
            .clone();
        let kinds: Vec<&str> = sprint.sessions.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(kinds, ["practice", "sprint_qualifying", "sprint", "qualifying"]);
    }

    /// The feed's fields this body deliberately drops, checked against the
    /// text of the response itself so a later "just carry it through" is a
    /// decision rather than a diff nobody reads.
    #[test]
    fn the_body_carries_none_of_the_fields_the_decision_dropped() {
        let written = serde_json::to_string(&season()).expect("the body serializes");
        for dropped in ["circuit", "Circuit", "country", "lat", "long", "url", "season", "round"] {
            assert!(
                !written.contains(dropped),
                "`{dropped}` is in the feed and deliberately not in the body"
            );
        }
    }

    /// A shape the parser does not recognise fails loudly, per race and per
    /// field, rather than writing something plausible.
    #[test]
    fn a_missing_required_field_is_named_with_its_round() {
        let mut value: Value = serde_json::from_str(FIXTURE).unwrap();
        value.pointer_mut("/MRData/RaceTable/Races/0").unwrap()
            .as_object_mut().unwrap().remove("raceName");
        assert_eq!(
            parse(&value.to_string()),
            Err(ScheduleError::Missing { round: "1".to_string(), field: "raceName" })
        );

        let mut value: Value = serde_json::from_str(FIXTURE).unwrap();
        value.pointer_mut("/MRData/RaceTable/Races/1/Circuit/Location").unwrap()
            .as_object_mut().unwrap().remove("locality");
        assert_eq!(
            parse(&value.to_string()),
            Err(ScheduleError::Missing {
                round: "2".to_string(),
                field: "Circuit.Location.locality",
            })
        );
    }

    /// A race with no `time` has no start instant, and a race with no start
    /// is not an event this lane can answer with — the same loud failure,
    /// named as the race start rather than as a bare field.
    #[test]
    fn a_race_with_no_start_time_is_refused() {
        let mut value: Value = serde_json::from_str(FIXTURE).unwrap();
        value.pointer_mut("/MRData/RaceTable/Races/0").unwrap()
            .as_object_mut().unwrap().remove("time");
        assert_eq!(
            parse(&value.to_string()),
            Err(ScheduleError::Missing { round: "1".to_string(), field: "the race start" })
        );
    }

    /// A session that is *present and malformed* is a shape change and
    /// fails; an **absent** session is silence, because the ladder is
    /// optional by construction — IndyCar, when its adapter lands, has a
    /// race start and no ladder at all.
    #[test]
    fn an_absent_session_is_silence_and_a_malformed_one_is_loud() {
        let mut value: Value = serde_json::from_str(FIXTURE).unwrap();
        value.pointer_mut("/MRData/RaceTable/Races/0").unwrap()
            .as_object_mut().unwrap().remove("SecondPractice");
        let season = parse(&value.to_string()).expect("an absent session is not a failure");
        assert_eq!(
            season.events[0].sessions.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(),
            ["Practice 1", "Practice 3", "Qualifying"]
        );

        let mut value: Value = serde_json::from_str(FIXTURE).unwrap();
        *value.pointer_mut("/MRData/RaceTable/Races/0/SecondPractice/time").unwrap() =
            Value::String("14:00 local".to_string());
        assert_eq!(
            parse(&value.to_string()),
            Err(ScheduleError::BadInstant {
                round: "1".to_string(),
                field: "SecondPractice",
                value: "2026-03-06 14:00 local".to_string(),
            })
        );
    }

    /// An empty race table is NOT read as an off-season. The feed answers
    /// `current` with the whole season all year, so zero races means its
    /// shape or its notion of "current" moved — and writing an empty season
    /// would have the pane answer "no races scheduled" indefinitely, which
    /// is a wrong answer rather than a slow one.
    #[test]
    fn an_empty_race_table_is_loud_rather_than_read_as_an_off_season() {
        let empty = serde_json::json!({"MRData": {"RaceTable": {"season": "2026", "Races": []}}});
        assert_eq!(parse(&empty.to_string()), Err(ScheduleError::NoRaces));
        assert_eq!(
            parse(&serde_json::json!({"MRData": {}}).to_string()),
            Err(ScheduleError::NotARaceTable)
        );
        assert!(matches!(parse("<html>403</html>"), Err(ScheduleError::NotJson(_))));
    }

    /// The instant arithmetic, on dates the days-from-civil algorithm has to
    /// get right: the epoch itself, a leap day, and a century that is not a
    /// leap year's neighbour.
    #[test]
    fn instants_are_utc_integer_arithmetic() {
        assert_eq!(instant_ms("1970-01-01", "00:00:00Z"), Some(0));
        assert_eq!(instant_ms("2026-03-08", "04:00:00Z"), Some(1_772_942_400_000));
        assert_eq!(instant_ms("2024-02-29", "12:30:00Z"), Some(1_709_209_800_000));
        assert_eq!(instant_ms("2026-03-08", "04:00Z"), Some(1_772_942_400_000));
    }

    /// A stamp this build will not guess at — most importantly one with no
    /// `Z`, since reading a local-looking time as UTC moves a race start by
    /// hours and nothing downstream could tell.
    #[test]
    fn a_stamp_without_a_utc_marker_is_refused_rather_than_assumed() {
        for (date, time) in [
            ("2026-03-08", "04:00:00"),
            ("2026-03-08", "04:00:00+01:00"),
            ("2026-03-08", "3:00PM ET"),
            ("2026-13-08", "04:00:00Z"),
            ("08/03/2026", "04:00:00Z"),
            ("2026-03-08", "24:30:00Z"),
        ] {
            assert_eq!(instant_ms(date, time), None, "{date} {time}");
        }
    }
}
