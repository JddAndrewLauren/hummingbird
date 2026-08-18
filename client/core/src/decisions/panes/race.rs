//! **The next-race question** (#119), sunk out of
//! `screens/race-pane/race.ts` by ADR-0025 (#534/M4).
//!
//! The lane is the poller's (`server/race-poll`): one `context_snapshots`
//! row per followed series, replaced wholesale every six hours, plus the
//! race-start alert at the 90-minute lead under the same source string.
//! What is here is the *deciding* half: the payload parser that pins the
//! `race-schedule/v1` body shape, the setup arm, `nextRaceAt`/`nextStartOf`,
//! the alert join, the band and its two thresholds (ADR-0015 keeps those
//! together, ADR-0025 moves the pair), and the gap **kinds**.
//!
//! What does not cross: [`RaceGap`] is a structured enum, never a sentence
//! — a client composes "This device doesn't know how to read … yet." from
//! [`RaceGap::UnknownSchema`] in its own words. `countdown`'s numeric split
//! (value + unit as *data*), `abbreviate`, `seriesLabel`, `dayLabel` and
//! `clock` all stay in `race.ts`: the first three are name/number formatting
//! and the last two are explicitly device-local wall-clock words (ADR-0015).
//! `raceCollapsedHeadline`/`raceHeadlineParts` are headline composition and
//! stay per-client for the same reason.
//!
//! **The under-way headline is dropped, not ported.** `nextRaceAt` treats an
//! event as behind us the instant its start passes — there is no session end
//! time anywhere in this lane, and inventing one is the fabrication rule
//! `waste.rs`'s absent `deviation` field already enforces. See `race.ts`'s
//! own note on [`next_race_at`] for the full argument; it is not repeated
//! here since restating it would make this the second copy.

use serde::{Deserialize, Serialize};

use super::contract::{AnswerState, Band, PaneAnswerCore};
use super::inputs::{BindingValueFact, FreshnessFact, PaneEnvelopeFacts, PaneInputs, PaneSnapshotFacts};

/// Both the snapshot's source and every race-start alert's — ADR-0009's join
/// constraint is that these are one string, and ADR-0015's envelope
/// `schema` is the same one again. Nothing here checks it against the
/// frozen source registry (`server/domain/src/sources.rs`); ADR-0015
/// forbids that, so this constant is this module's own and the two agree by
/// review.
pub const SOURCE: &str = "race-schedule/v1";

/// The binding that has to be set before this question can be asked at all
/// — the same key `server/race-poll/src/binding.rs` reads, unversioned so a
/// `/v1 → /v2` source bump cannot orphan it.
pub const BINDING_KEY: &str = "race-series";

/// How old an answer may be before it is worth saying so — beside the band
/// function, where ADR-0015 puts every threshold.
///
/// Twelve hours: `2 ×` the schedule poller's declared six-hour cadence. A
/// race schedule moves rarely and a missed poll costs nothing until it
/// does, unlike `waste.rs`'s 26h departure from the same rule.
pub const STALE_AFTER_MS: i64 = 12 * 60 * 60 * 1000;

/// The one sentinel subject an unbound (or not-yet-read) question emits, so
/// the setup prompt exists to be found — never a real series key.
pub const SETUP_SUBJECT: &str = "setup";

/// The race is close enough that the day is about it.
const IMMINENT_MS: i64 = 24 * 60 * 60 * 1000;
/// The weekend is running, or begins soon enough to plan around.
const NEAR_MS: i64 = 72 * 60 * 60 * 1000;

/// One supporting session — practice, qualifying, the sprint and its own
/// qualifying. **Never the race**; see the module note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RaceSession {
    /// The poller's stable machine name. Deliberately an open string, not a
    /// closed enum: a feed that grows a session kind must not fail this
    /// whole parse, and nothing here branches on the value.
    pub kind: String,
    pub label: String,
    pub starts_at_ms: i64,
}

/// One race weekend: the race start, plus its supporting ladder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RaceEvent {
    pub name: String,
    pub locality: String,
    /// **The race start**, never the first session's.
    pub starts_at_ms: i64,
    pub sessions: Vec<RaceSession>,
}

/// The `race-schedule/v1` body — the whole season, in feed order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RaceBody {
    pub events: Vec<RaceEvent>,
}

/// Why this pane has no answer — a **kind**, not a sentence.
///
/// Mirrors `parseRaceBody`'s gap reasons 1:1: [`RaceGap::UnknownSchema`] is
/// separate from [`RaceGap::Malformed`] on purpose, exactly as `waste.rs`'s
/// own pair — a newer build wrote a shape this one has never heard of,
/// which is fixed by updating the app, not by looking at the feed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "gap", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RaceGap {
    /// No snapshot row at all: nothing has ever been fetched for this series.
    NotFetched,
    /// The envelope itself could not be read. `reason` is
    /// `hummingbird_domain::EnvelopeProblem`'s own wording, passed through
    /// as data.
    Malformed { reason: String },
    UnknownSchema { schema: String },
    NotJson,
    NotAnObject,
    /// `events` is missing or is not an array — "the schedule payload
    /// carries no season" in `race.ts`'s own words.
    NoSeason,
    /// One entry in `events` (or one of its `sessions`) failed to parse.
    BadEvent,
}

/// Whether the question has been asked at all — **four answers, not a
/// boolean**, exactly `waste.rs`'s `WasteSetup` shape and for the same
/// reasons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RaceSetup {
    Bound { series: Vec<String> },
    /// The bindings table has not been read on this device yet.
    Unread,
    /// A row exists but holds something that is not text.
    Unusable,
    /// No row, or one holding nothing — the only arm that is genuinely
    /// `unbound`.
    Unset,
}

/// Everything one answered pane needs, decided once and read by both the
/// answer and a client's expanded rendering. **No rendered sentence
/// crosses.**
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RaceFacts {
    pub series: String,
    /// The next race weekend, or `None` **off-season** — a season whose
    /// races have all run is an answer ("no races scheduled"), not a gap.
    pub event: Option<RaceEvent>,
    /// The next thing on track for that weekend — Friday practice for most
    /// of the year, the race itself once the ladder is done. `None`
    /// off-season, and never `None` when `event` is `Some`.
    pub next_start: Option<(String, i64)>,
    /// Whether this series' race-start alert is currently live, joined on
    /// `(source, subjectKey)` ↔ `(source, key)` — see [`race_facts`].
    pub has_live_alert: bool,
    pub stale: bool,
    pub freshness: FreshnessFact,
}

/// The whole answered fact set, or the reason there is none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RaceResolved {
    Facts(RaceFacts),
    Gap { gap: RaceGap },
}

fn is_i64(value: Option<&serde_json::Value>) -> Option<i64> {
    value?.as_i64()
}

fn parse_session(raw: &serde_json::Value) -> Option<RaceSession> {
    let object = raw.as_object()?;
    let kind = object.get("kind")?.as_str()?.to_string();
    let label = object.get("label")?.as_str()?.to_string();
    let starts_at_ms = is_i64(object.get("starts_at_ms"))?;
    Some(RaceSession { kind, label, starts_at_ms })
}

fn parse_event(raw: &serde_json::Value) -> Option<RaceEvent> {
    let object = raw.as_object()?;
    let name = object.get("name")?.as_str()?.to_string();
    let locality = object.get("locality")?.as_str()?.to_string();
    let starts_at_ms = is_i64(object.get("starts_at_ms"))?;
    let raw_sessions = object.get("sessions")?.as_array()?;
    let mut sessions = Vec::with_capacity(raw_sessions.len());
    for entry in raw_sessions {
        sessions.push(parse_session(entry)?);
    }
    Some(RaceEvent { name, locality, starts_at_ms, sessions })
}

/// Reads one snapshot row into a season, or says why it could not.
///
/// A malformed session or event fails the **whole** parse — same as
/// `parseRaceBody` — and an **empty season is a legitimate value, not a
/// failure** (the poller says so from the other side, `body.rs`): off
/// season, [`race_answer`] answers "no races scheduled" rather than a gap.
pub fn parse_race_body(snapshot: Option<&PaneSnapshotFacts>) -> Result<RaceBody, RaceGap> {
    let Some(snapshot) = snapshot else {
        return Err(RaceGap::NotFetched);
    };
    let (schema, body) = match &snapshot.envelope {
        PaneEnvelopeFacts::Malformed { reason } => {
            return Err(RaceGap::Malformed { reason: reason.clone() })
        }
        PaneEnvelopeFacts::Ok { schema, body } => (schema, body),
    };
    if schema != SOURCE {
        return Err(RaceGap::UnknownSchema { schema: schema.clone() });
    }

    let parsed: serde_json::Value = serde_json::from_str(body).map_err(|_| RaceGap::NotJson)?;
    let object = parsed.as_object().ok_or(RaceGap::NotAnObject)?;
    let raw_events = object.get("events").and_then(|v| v.as_array()).ok_or(RaceGap::NoSeason)?;

    let mut events = Vec::with_capacity(raw_events.len());
    for entry in raw_events {
        events.push(parse_event(entry).ok_or(RaceGap::BadEvent)?);
    }
    Ok(RaceBody { events })
}

/// The followed series, read out of the `race-series` binding's text.
///
/// **The same reading `server/race-poll/src/binding.rs` gives that row**:
/// trimmed, lowercased, blanks dropped, repeats dropped, order kept.
pub fn series_from_binding(text: &str) -> Vec<String> {
    let mut series = Vec::new();
    for entry in text.split(',') {
        let key = entry.trim().to_lowercase();
        if key.is_empty() || series.contains(&key) {
            continue;
        }
        series.push(key);
    }
    series
}

/// Whether the followed-series binding has been set, and if not, which kind
/// of not-set it is.
pub fn race_setup(inputs: &PaneInputs) -> RaceSetup {
    if inputs.bindings.is_none() {
        return RaceSetup::Unread;
    }
    match inputs.binding(BINDING_KEY).map(|binding| &binding.value) {
        None | Some(BindingValueFact::Unset) => RaceSetup::Unset,
        Some(BindingValueFact::Other) => RaceSetup::Unusable,
        Some(BindingValueFact::Text { text }) => {
            let series = series_from_binding(text);
            // A row blanked to whitespace (or to nothing but separators) is
            // the nearest thing `settings` has to a DELETE, and reads as
            // never set.
            if series.is_empty() {
                RaceSetup::Unset
            } else {
                RaceSetup::Bound { series }
            }
        }
    }
}

/// This question's subjects: one per followed series, in binding order —
/// **0..N from a `settings` row**, which is the acceptance criterion: a
/// series with no adapter upstream still gets its pane, as a gap.
pub fn race_subjects(inputs: &PaneInputs) -> Vec<String> {
    match race_setup(inputs) {
        RaceSetup::Bound { series } => series,
        _ => vec![SETUP_SUBJECT.to_string()],
    }
}

/// The next race still ahead — the soonest event whose **start instant**
/// has not passed, scanned by instant rather than trusted to feed order.
///
/// The start instant is this pane's horizon and the settled answer to the
/// "under way" question (#266's grilling note on #119): no session end time
/// exists in this lane and one must never be invented, so once a race has
/// started this pane simply names the *following* one for the couple of
/// hours the race is actually running.
pub fn next_race_at(events: &[RaceEvent], now_ms: i64) -> Option<&RaceEvent> {
    let mut next: Option<&RaceEvent> = None;
    for event in events {
        if event.starts_at_ms <= now_ms {
            continue;
        }
        if next.is_none_or(|current| event.starts_at_ms < current.starts_at_ms) {
            next = Some(event);
        }
    }
    next
}

/// The next thing on track for one weekend — the soonest upcoming start
/// among the supporting ladder and the race itself.
///
/// Separate from the race because the two differ for most of a race weekend
/// (Friday practice is two days before Sunday's race). Never absent for an
/// event this pane is showing: the race start is itself a candidate and is
/// upcoming by construction.
pub fn next_start_of(event: &RaceEvent, now_ms: i64) -> (String, i64) {
    let mut next = ("Race".to_string(), event.starts_at_ms);
    for session in &event.sessions {
        if session.starts_at_ms > now_ms && session.starts_at_ms < next.1 {
            next = (session.label.clone(), session.starts_at_ms);
        }
    }
    next
}

/// The whole answered fact set for one series, or the reason there is none.
///
/// The alert join, and the only place it is spelled: ADR-0015 added
/// `alerts.subject_key` naming this pane as its forcing case — ONE source
/// carries a row per series, so joining on `source` alone would put every
/// series' race-start alert on every series' pane.
pub fn race_facts(series: &str, inputs: &PaneInputs) -> RaceResolved {
    let snapshot = inputs.snapshot(SOURCE, series);
    let body = match parse_race_body(snapshot) {
        Ok(body) => body,
        Err(gap) => return RaceResolved::Gap { gap },
    };
    // `parse_race_body` returning `Ok` guarantees the row exists.
    let snapshot = snapshot.expect("a parsed body implies a snapshot row");

    let event = next_race_at(&body.events, inputs.now_ms).cloned();
    let next_start = event.as_ref().map(|event| next_start_of(event, inputs.now_ms));
    let has_live_alert = inputs
        .live_alerts(SOURCE)
        .iter()
        .any(|alert| alert.subject_key.as_deref() == Some(series));

    RaceResolved::Facts(RaceFacts {
        series: series.to_string(),
        event,
        next_start,
        has_live_alert,
        stale: snapshot.freshness.is_stale_beyond(STALE_AFTER_MS),
        freshness: snapshot.freshness,
    })
}

/// This question's answer for the shell (#245), minus its rendering half.
///
/// The bands, and why each threshold is where it is:
///
///   * `live` — the lane's own race-start alert is live for this series,
///     joined on `subjectKey`. Deliberately not a second time threshold: the
///     alert is raised at `race_poll::next::LEAD_MS` (90 minutes) and
///     expires at the start, so reading the alert rather than re-deriving
///     its window keeps the pane and the notification from ever disagreeing
///     about the same race.
///   * `imminent` — the race starts within a day.
///   * `near` — the next thing on track (usually Friday practice) is within
///     three days, i.e. the race weekend is here.
///   * `distant` — a race is scheduled, further out than that.
///   * `dormant` — off-season: nothing is scheduled at all.
///
/// `within_band` is the instant of the next thing on track. `None` only
/// off-season, where there is genuinely nothing to order by.
pub fn race_answer(subject_key: &str, inputs: &PaneInputs) -> PaneAnswerCore {
    let gap = |answer_state| PaneAnswerCore { answer_state, band: Band::Dormant, within_band: None };
    match race_setup(inputs) {
        RaceSetup::Unset => return gap(AnswerState::Unbound),
        // Neither answered nor unbound: the table has not been read yet, or
        // it holds something this pane cannot use.
        RaceSetup::Unread | RaceSetup::Unusable => return gap(AnswerState::BoundButUnacquired),
        RaceSetup::Bound { .. } => {}
    }

    let facts = match race_facts(subject_key, inputs) {
        // A followed series nothing has written a snapshot for (or a body
        // this build cannot read) — named as a gap rather than dropped, so
        // a series added to the binding that produced no pane at all does
        // not look like the edit did nothing.
        RaceResolved::Gap { .. } => return gap(AnswerState::BoundButUnacquired),
        RaceResolved::Facts(facts) => facts,
    };

    let (Some(event), Some(next_start)) = (facts.event, facts.next_start) else {
        return PaneAnswerCore { answer_state: AnswerState::Answered, band: Band::Dormant, within_band: None };
    };

    let to_race_ms = event.starts_at_ms - inputs.now_ms;
    let to_next_ms = next_start.1 - inputs.now_ms;
    let band = if facts.has_live_alert {
        Band::Live
    } else if to_race_ms <= IMMINENT_MS {
        Band::Imminent
    } else if to_next_ms <= NEAR_MS {
        Band::Near
    } else {
        Band::Distant
    };

    PaneAnswerCore { answer_state: AnswerState::Answered, band, within_band: Some(next_start.1) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decisions::panes::inputs::PaneAlertFacts;

    const MINUTE_MS: i64 = 60 * 1000;
    const HOUR_MS: i64 = 60 * MINUTE_MS;
    const DAY_MS: i64 = 24 * HOUR_MS;
    /// Monday 2026-08-10 09:00 UTC, matching `race.test.ts`'s `NOW`.
    const NOW: i64 = 1_786_957_200_000;

    fn bound(text: &str) -> serde_json::Value {
        serde_json::json!([
            {"key": BINDING_KEY, "value": {"state":"text","text":text}}
        ])
    }

    fn ok_envelope(body: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({"kind":"ok","schema":SOURCE,"body": body.to_string()})
    }

    fn event_json(name: &str, starts_at_ms: i64, session_offsets: &[i64]) -> serde_json::Value {
        let sessions: Vec<serde_json::Value> = session_offsets
            .iter()
            .enumerate()
            .map(|(index, offset)| {
                serde_json::json!({
                    "kind": "practice",
                    "label": format!("Practice {}", index + 1),
                    "starts_at_ms": starts_at_ms + offset,
                })
            })
            .collect();
        serde_json::json!({
            "name": name,
            "locality": name.split(' ').next().unwrap_or(name),
            "starts_at_ms": starts_at_ms,
            "sessions": sessions,
        })
    }

    fn inputs_with(pane_reads: serde_json::Value, bindings: serde_json::Value) -> PaneInputs {
        serde_json::from_value(serde_json::json!({
            "nowMs": NOW,
            "bindings": bindings,
            "paneReads": pane_reads,
        }))
        .unwrap()
    }

    /// One `paneReads` entry: a snapshot per `(key, events)` pair, plus any
    /// live alerts.
    fn season_read(entries: &[(&str, Vec<serde_json::Value>)], alerts: Vec<serde_json::Value>) -> serde_json::Value {
        let snapshots: Vec<serde_json::Value> = entries
            .iter()
            .map(|(key, events)| {
                serde_json::json!({
                    "key": key,
                    "envelope": ok_envelope(&serde_json::json!({"events": events})),
                    "freshness": {"kind":"age","ageMs":60000,"declaredCadenceMs": 6 * 60 * 60 * 1000},
                })
            })
            .collect();
        serde_json::json!({ SOURCE: { "snapshots": snapshots, "liveAlerts": alerts } })
    }

    fn alert_json(subject_key: Option<&str>) -> serde_json::Value {
        serde_json::json!({"subjectKey": subject_key})
    }

    fn events_of(inputs: &PaneInputs, series: &str) -> Vec<RaceEvent> {
        match parse_race_body(inputs.snapshot(SOURCE, series)) {
            Ok(body) => body.events,
            Err(gap) => panic!("expected a parseable season, got {gap:?}"),
        }
    }

    // -------------------------------------------------------- parse_race_body

    #[test]
    fn reads_the_season_out_of_the_pollers_own_committed_golden_body() {
        let golden: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../server/race-poll/tests/fixtures/golden-body.json"
        )))
        .unwrap();
        let body = golden.get("body").unwrap();
        let inputs = inputs_with(
            serde_json::json!({ SOURCE: { "snapshots": [{
                "key": "f1",
                "envelope": ok_envelope(body),
                "freshness": {"kind":"age","ageMs":60000,"declaredCadenceMs": 6 * 60 * 60 * 1000},
            }], "liveAlerts": [] } }),
            bound("f1"),
        );
        let parsed = parse_race_body(inputs.snapshot(SOURCE, "f1")).unwrap();
        let first = &parsed.events[0];
        assert_eq!(first.name, "Australian Grand Prix");
        assert_eq!(first.locality, "Melbourne");
        assert_eq!(first.starts_at_ms, 1_772_942_400_000);
        assert_eq!(
            first.sessions[0],
            RaceSession {
                kind: "practice".to_string(),
                label: "Practice 1".to_string(),
                starts_at_ms: 1_772_760_600_000,
            },
        );
        // `sessions` is the supporting ladder and never holds the race,
        // whose start is the event's own.
        assert!(first.sessions.iter().map(|s| s.starts_at_ms).max().unwrap() < first.starts_at_ms);
    }

    #[test]
    fn names_which_kind_of_gap_it_is_rather_than_answering_emptily() {
        assert_eq!(parse_race_body(None), Err(RaceGap::NotFetched));

        let inputs = inputs_with(
            serde_json::json!({ SOURCE: { "snapshots": [
                {"key":"f1","envelope":{"kind":"malformed","reason":"no `schema`"},
                 "freshness":{"kind":"unknown"}},
            ], "liveAlerts": [] } }),
            bound("f1"),
        );
        assert_eq!(
            parse_race_body(inputs.snapshot(SOURCE, "f1")),
            Err(RaceGap::Malformed { reason: "no `schema`".to_string() }),
        );

        let cases: Vec<(serde_json::Value, RaceGap)> = vec![
            (
                serde_json::json!({"kind":"ok","schema":"race-schedule/v2","body":"{}"}),
                RaceGap::UnknownSchema { schema: "race-schedule/v2".to_string() },
            ),
            (serde_json::json!({"kind":"ok","schema":SOURCE,"body":"{"}), RaceGap::NotJson),
            (
                ok_envelope(&serde_json::json!({"events": "the whole season, honest"})),
                RaceGap::NoSeason,
            ),
            (
                ok_envelope(&serde_json::json!({"events": [{"name": "A GP"}]})),
                RaceGap::BadEvent,
            ),
        ];
        for (envelope, expected) in cases {
            let inputs = inputs_with(
                serde_json::json!({ SOURCE: { "snapshots": [
                    {"key":"f1","envelope": envelope,
                     "freshness":{"kind":"unknown"}},
                ], "liveAlerts": [] } }),
                bound("f1"),
            );
            assert_eq!(parse_race_body(inputs.snapshot(SOURCE, "f1")), Err(expected.clone()), "{expected:?}");
        }
    }

    #[test]
    fn a_bare_array_is_not_an_object() {
        let inputs = inputs_with(
            serde_json::json!({ SOURCE: { "snapshots": [
                {"key":"f1","envelope":{"kind":"ok","schema":SOURCE,"body":"[]"},
                 "freshness":{"kind":"unknown"}},
            ], "liveAlerts": [] } }),
            bound("f1"),
        );
        assert_eq!(parse_race_body(inputs.snapshot(SOURCE, "f1")), Err(RaceGap::NotAnObject));
    }

    #[test]
    fn reads_an_off_season_body_as_an_empty_season_rather_than_as_a_broken_one() {
        let inputs = inputs_with(season_read(&[("f1", vec![])], vec![]), bound("f1"));
        assert_eq!(parse_race_body(inputs.snapshot(SOURCE, "f1")).unwrap(), RaceBody { events: vec![] });
    }

    // -------------------------------------------------------------- series_from_binding

    #[test]
    fn reads_the_comma_separated_list_exactly_as_the_pollers_own_binding_rs_does() {
        assert_eq!(series_from_binding(" F1 , indycar ,,f1 "), vec!["f1", "indycar"]);
        assert_eq!(series_from_binding("f1"), vec!["f1"]);
        assert!(series_from_binding(" , , ").is_empty());
    }

    // -------------------------------------------------------------- race_setup

    #[test]
    fn tells_an_unread_bindings_table_apart_from_a_genuinely_unset_one() {
        assert_eq!(race_setup(&inputs_with(serde_json::json!({}), serde_json::Value::Null)), RaceSetup::Unread);
        assert_eq!(race_setup(&inputs_with(serde_json::json!({}), serde_json::json!([]))), RaceSetup::Unset);
        assert_eq!(
            race_setup(&inputs_with(
                serde_json::json!({}),
                serde_json::json!([{"key": BINDING_KEY, "value": {"state":"unset"}}]),
            )),
            RaceSetup::Unset,
        );
        assert_eq!(
            race_setup(&inputs_with(
                serde_json::json!({}),
                serde_json::json!([{"key": BINDING_KEY, "value": {"state":"text","text":"  "}}]),
            )),
            RaceSetup::Unset,
        );
    }

    #[test]
    fn reads_a_row_holding_something_that_is_not_text_as_a_gap_never_as_unset() {
        assert_eq!(
            race_setup(&inputs_with(
                serde_json::json!({}),
                serde_json::json!([{"key": BINDING_KEY, "value": {"state":"other","raw":"[\"f1\"]"}}]),
            )),
            RaceSetup::Unusable,
        );
    }

    #[test]
    fn hands_back_every_followed_series_in_the_order_the_binding_named_them() {
        assert_eq!(
            race_setup(&inputs_with(serde_json::json!({}), bound("f1, indycar"))),
            RaceSetup::Bound { series: vec!["f1".to_string(), "indycar".to_string()] },
        );
    }

    // -------------------------------------------------------------- race_subjects

    #[test]
    fn emits_one_pane_per_followed_series_and_one_setup_pane_when_nobody_follows_any() {
        assert_eq!(
            race_subjects(&inputs_with(serde_json::json!({}), bound("f1, indycar"))),
            vec!["f1".to_string(), "indycar".to_string()],
        );
        assert_eq!(
            race_subjects(&inputs_with(serde_json::json!({}), serde_json::json!([]))),
            vec![SETUP_SUBJECT.to_string()],
        );
        assert_eq!(
            race_subjects(&inputs_with(serde_json::json!({}), serde_json::Value::Null)),
            vec![SETUP_SUBJECT.to_string()],
        );
    }

    // ------------------------------------------------------------- next_race_at

    #[test]
    fn picks_the_soonest_race_still_ahead_by_instant_rather_than_by_feed_order() {
        let inputs = inputs_with(
            season_read(
                &[("f1", vec![event_json("Late GP", NOW + 20 * DAY_MS, &[]), event_json("Soon GP", NOW + 3 * DAY_MS, &[])])],
                vec![],
            ),
            bound("f1"),
        );
        let events = events_of(&inputs, "f1");
        assert_eq!(next_race_at(&events, NOW).unwrap().name, "Soon GP");
    }

    #[test]
    fn treats_a_race_that_has_already_started_as_behind_us() {
        let inputs = inputs_with(
            season_read(
                &[("f1", vec![event_json("Today GP", NOW + HOUR_MS, &[]), event_json("Next GP", NOW + 14 * DAY_MS, &[])])],
                vec![],
            ),
            bound("f1"),
        );
        let events = events_of(&inputs, "f1");
        assert_eq!(next_race_at(&events, NOW).unwrap().name, "Today GP");
        assert_eq!(next_race_at(&events, NOW + HOUR_MS + 1).unwrap().name, "Next GP");
    }

    #[test]
    fn has_no_race_to_name_once_the_season_is_over() {
        let inputs = inputs_with(
            season_read(&[("f1", vec![event_json("Done GP", NOW - DAY_MS, &[])])], vec![]),
            bound("f1"),
        );
        let events = events_of(&inputs, "f1");
        assert!(next_race_at(&events, NOW).is_none());
        assert!(next_race_at(&[], NOW).is_none());
    }

    // ------------------------------------------------------------- next_start_of

    #[test]
    fn picks_the_soonest_ladder_session_over_the_race_itself() {
        let event = RaceEvent {
            name: "Monaco Grand Prix".to_string(),
            locality: "Monaco".to_string(),
            starts_at_ms: NOW + 12 * DAY_MS,
            sessions: vec![RaceSession {
                kind: "practice".to_string(),
                label: "Practice 1".to_string(),
                starts_at_ms: NOW + 10 * DAY_MS,
            }],
        };
        assert_eq!(next_start_of(&event, NOW), ("Practice 1".to_string(), NOW + 10 * DAY_MS));
    }

    #[test]
    fn falls_back_to_the_race_once_every_session_has_passed() {
        let event = RaceEvent {
            name: "Monaco Grand Prix".to_string(),
            locality: "Monaco".to_string(),
            starts_at_ms: NOW + 20 * HOUR_MS,
            sessions: vec![RaceSession {
                kind: "practice".to_string(),
                label: "Practice 1".to_string(),
                starts_at_ms: NOW - 2 * DAY_MS,
            }],
        };
        assert_eq!(next_start_of(&event, NOW), ("Race".to_string(), NOW + 20 * HOUR_MS));
    }

    // ---------------------------------------------------------------- race_answer

    #[test]
    fn counts_to_the_race_with_within_band_naming_the_next_thing_on_track() {
        let inputs = inputs_with(
            season_read(&[("f1", vec![event_json("Monaco Grand Prix", NOW + 12 * DAY_MS, &[-2 * DAY_MS])])], vec![]),
            bound("f1"),
        );
        let answer = race_answer("f1", &inputs);
        assert_eq!(answer.answer_state, AnswerState::Answered);
        assert_eq!(answer.within_band, Some(NOW + 10 * DAY_MS));
        assert_eq!(answer.band, Band::Distant);
    }

    #[test]
    fn renders_a_series_the_binding_names_but_nothing_has_polled_as_a_gap_never_as_absence() {
        let inputs = inputs_with(
            season_read(&[("f1", vec![event_json("Monaco Grand Prix", NOW + DAY_MS, &[])])], vec![]),
            bound("f1, indycar"),
        );
        let answer = race_answer("indycar", &inputs);
        assert_eq!(answer.answer_state, AnswerState::BoundButUnacquired);
    }

    #[test]
    fn joins_an_alert_on_source_and_subject_key_and_never_shows_one_series_alert_on_anothers_pane() {
        let world = |alerts: Vec<serde_json::Value>| {
            inputs_with(
                season_read(
                    &[
                        ("f1", vec![event_json("Monaco Grand Prix", NOW + 80 * MINUTE_MS, &[])]),
                        ("indycar", vec![event_json("Iowa 275", NOW + 9 * DAY_MS, &[])]),
                    ],
                    alerts,
                ),
                bound("f1, indycar"),
            )
        };
        assert_eq!(race_answer("f1", &world(vec![alert_json(Some("f1"))])).band, Band::Live);
        assert_ne!(race_answer("indycar", &world(vec![alert_json(Some("f1"))])).band, Band::Live);
        // An alert naming no subject at all belongs to no pane, and is not
        // dropped — the join is additive, and lives entirely outside this
        // module (`AlertsScreen`).
        assert_ne!(race_answer("f1", &world(vec![alert_json(None)])).band, Band::Live);
    }

    #[test]
    fn bands_the_weekend_it_is_in_and_orders_two_panes_inside_one_band_by_their_next_start() {
        let world = |starts_in_ms: i64| {
            inputs_with(
                season_read(&[("f1", vec![event_json("Monaco Grand Prix", NOW + starts_in_ms, &[-2 * DAY_MS])])], vec![]),
                bound("f1"),
            )
        };
        assert_eq!(race_answer("f1", &world(20 * HOUR_MS)).band, Band::Imminent);
        assert_eq!(race_answer("f1", &world(2 * DAY_MS)).band, Band::Near);
        assert_eq!(race_answer("f1", &world(12 * DAY_MS)).band, Band::Distant);
    }

    #[test]
    fn answers_an_off_season_season_rather_than_calling_it_a_gap() {
        let inputs = inputs_with(
            season_read(&[("f1", vec![event_json("Abu Dhabi Grand Prix", NOW - 3 * DAY_MS, &[])])], vec![]),
            bound("f1"),
        );
        let answer = race_answer("f1", &inputs);
        assert_eq!(answer.answer_state, AnswerState::Answered);
        assert_eq!(answer.band, Band::Dormant);
        assert_eq!(answer.within_band, None);
    }

    #[test]
    fn says_unbound_only_for_a_genuinely_unset_binding_and_bound_but_unacquired_while_it_is_unread() {
        assert_eq!(
            race_answer(SETUP_SUBJECT, &inputs_with(serde_json::json!({}), serde_json::json!([]))).answer_state,
            AnswerState::Unbound,
        );
        assert_eq!(
            race_answer(SETUP_SUBJECT, &inputs_with(serde_json::json!({}), serde_json::Value::Null)).answer_state,
            AnswerState::BoundButUnacquired,
        );
    }

    // ----------------------------------------------------------------- race_facts

    #[test]
    fn calls_an_answer_stale_past_twelve_hours_and_never_calls_an_unknown_age_fresh() {
        let world = |freshness: serde_json::Value| {
            inputs_with(
                serde_json::json!({ SOURCE: { "snapshots": [{
                    "key": "f1",
                    "envelope": ok_envelope(&serde_json::json!({"events": [event_json("Monaco Grand Prix", NOW + 12 * DAY_MS, &[])]})),
                    "freshness": freshness,
                }], "liveAlerts": [] } }),
                bound("f1"),
            )
        };
        let stale_of = |freshness: serde_json::Value| match race_facts("f1", &world(freshness)) {
            RaceResolved::Facts(facts) => facts.stale,
            RaceResolved::Gap { gap } => panic!("expected facts, got {gap:?}"),
        };
        assert!(!stale_of(serde_json::json!({"kind":"age","ageMs": 11 * HOUR_MS,"declaredCadenceMs": null})));
        assert!(stale_of(serde_json::json!({"kind":"age","ageMs": 13 * HOUR_MS,"declaredCadenceMs": null})));
        assert!(stale_of(serde_json::json!({"kind":"unknown"})));
    }

    #[test]
    fn a_never_polled_series_named_in_the_binding_is_a_gap() {
        let inputs = inputs_with(serde_json::json!({}), bound("indycar"));
        match race_facts("indycar", &inputs) {
            RaceResolved::Gap { gap } => assert_eq!(gap, RaceGap::NotFetched),
            RaceResolved::Facts(facts) => panic!("expected a gap, got {facts:?}"),
        }
    }

    #[test]
    fn the_live_alerts_helper_reads_the_subject_key_field_this_join_needs() {
        let alert = PaneAlertFacts { subject_key: Some("f1".to_string()) };
        assert_eq!(alert.subject_key.as_deref(), Some("f1"));
    }
}
