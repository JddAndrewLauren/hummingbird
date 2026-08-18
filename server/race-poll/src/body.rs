//! The `context_snapshots.payload` `race-schedule-poll` writes: ADR-0015's
//! envelope around a `race-schedule/v1` body — and the same body read back
//! by `race-alert-poll`, which holds no schedule of its own.
//!
//! **This is one half of a cross-language contract.** The other half is
//! `client/core/src/decisions/panes/race.rs`'s `parse_race_body` (sunk out
//! of `client/web/src/screens/race-pane/race.ts` at #534), so
//! `tests/fixtures/golden-body.json` stands in for the wire fixture: the
//! exact envelope this poller emits from the committed Jolpica response,
//! byte-compared by `tests/golden.rs`. `tests/contract.rs` asserts the
//! literal snake_case keys against `race.rs`'s own text, exactly as
//! `server/city-waste`'s `contract.rs` asserts them against `waste.rs`'s.
//! Nothing mechanical connects the two sides — the body inside the
//! envelope is deliberately unfrozen and opaque to the server — so a rename
//! on either side compiles and passes on both.
//!
//! # What the shape says, and what it deliberately does not carry
//!
//! **`starts_at_ms` on the event is the race start; `sessions` holds only
//! the *supporting* sessions and never the race.** That is what expresses
//! the F1/IndyCar asymmetry in the type: IndyCar, when its adapter lands,
//! has a race start and no ladder, and if the race lived inside the optional
//! ladder such an event would have no start at all.
//!
//! **Whole season, unfiltered, in feed order.** Not "next N" — filtering to
//! what is upcoming is a read-time answer and ADR-0002 says answers are
//! never stored. It also keeps the body reproducible in tests, since no
//! clock touches it.
//!
//! **Epoch ms, not ISO dates.** A race start is an *instant*. Contrast
//! `city-waste`, whose collection day is genuinely a civil date and carries
//! a `zone` for that reason. This body carries **no `zone`**, per ADR-0015's
//! device-local decision — and that is why this crate needs no tzdb.
//!
//! **No `ends_at_ms`. The feed has none. Do not invent one.**
//!
//! Dropped from the feed because nothing reads them: `circuit`, `country`,
//! lat/long, the wikipedia `url`, `season` and `round`. Dropped from the
//! prototype's guessed payload: `series`/`seriesLabel` (the row `key` **is**
//! the series) and `feed` (the adapter header documents the choice; a
//! provenance field inside a payload is a fact that can go stale).

use serde::{Deserialize, Serialize};

use hummingbird_domain::{SnapshotEnvelope, RACE_SCHEDULE_V1};

/// How often `race-schedule-poll` says it runs, for `Freshness`'s declared
/// cadence. **Must match `.github/workflows/race-schedule-poll.yml`'s
/// cron**; the workflow's header says so from the other side.
///
/// This is the number the two-cron split exists to protect: at 6h,
/// ADR-0015's `2 × cadence` staleness rule reads 12h and works unchanged.
/// `race-alert-poll`'s own `*/15` cron is deliberately **not** a declared
/// cadence — it writes no snapshot, so it declares nothing.
pub const POLLED_EVERY_MS: i64 = 6 * 60 * 60 * 1000;

/// One supporting session on the weekend's ladder — practice, qualifying,
/// the sprint and its own qualifying. Never the race itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// A stable machine name (`practice`, `qualifying`, `sprint`,
    /// `sprint_qualifying`) — what a pane groups or filters on.
    pub kind: String,
    /// What to put on screen (`Practice 1`). Separate from `kind` because
    /// three practices share one kind and each needs its own words.
    pub label: String,
    pub starts_at_ms: i64,
}

/// One race weekend: the race start, plus the supporting ladder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceEvent {
    pub name: String,
    pub locality: String,
    /// **The race start**, never the first session's — see the module note.
    pub starts_at_ms: i64,
    pub sessions: Vec<Session>,
}

/// The `race-schedule/v1` body. Field names are the wire contract; see the
/// module note above before renaming one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceScheduleBody {
    pub events: Vec<RaceEvent>,
}

/// Why a stored snapshot could not be read back as a season.
/// `race-alert-poll` holds no schedule of its own, so this is its whole
/// input surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyProblem {
    /// The ADR-0015 envelope itself did not parse, in its own words.
    Envelope(String),
    /// The envelope parsed but its `schema` is not this poller's — reading
    /// someone else's payload as a season would answer plausibly and
    /// wrongly.
    WrongSchema(String),
    /// The envelope's `body` is not a `race-schedule/v1` body.
    Body(String),
}

impl std::fmt::Display for BodyProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BodyProblem::Envelope(reason) => write!(f, "snapshot envelope: {reason}"),
            BodyProblem::WrongSchema(schema) => {
                write!(f, "snapshot carries `{schema}`, not `{RACE_SCHEDULE_V1}`")
            }
            BodyProblem::Body(reason) => write!(f, "snapshot body: {reason}"),
        }
    }
}

impl RaceScheduleBody {
    /// Wraps the body in ADR-0015's envelope — the `payload` value of a
    /// `POST /api/snapshots`.
    pub fn envelope(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": RACE_SCHEDULE_V1,
            "polled_every_ms": POLLED_EVERY_MS,
            "body": self,
        })
    }

    /// Reads a season back out of a stored snapshot's `payload` text
    /// (`GET /api/snapshots`'s `payload` field, exactly as written by
    /// [`RaceScheduleBody::envelope`]).
    pub fn from_payload(payload: &str) -> Result<RaceScheduleBody, BodyProblem> {
        let envelope =
            SnapshotEnvelope::parse(payload).map_err(|p| BodyProblem::Envelope(p.to_string()))?;
        if envelope.schema != RACE_SCHEDULE_V1 {
            return Err(BodyProblem::WrongSchema(envelope.schema));
        }
        serde_json::from_str(&envelope.body).map_err(|e| BodyProblem::Body(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_race() -> RaceScheduleBody {
        RaceScheduleBody {
            events: vec![RaceEvent {
                name: "Australian Grand Prix".to_string(),
                locality: "Melbourne".to_string(),
                starts_at_ms: 1_772_942_400_000,
                sessions: vec![Session {
                    kind: "practice".to_string(),
                    label: "Practice 1".to_string(),
                    starts_at_ms: 1_772_767_800_000,
                }],
            }],
        }
    }

    /// The envelope half of the contract: `SnapshotEnvelope::parse` is the
    /// exact check `POST /api/snapshots` runs, so anything this poller can
    /// build must survive it — and the declared cadence must be the six
    /// hours the split exists to keep.
    #[test]
    fn the_envelope_this_poller_builds_passes_the_authoritys_own_parse() {
        let payload = one_race().envelope().to_string();
        let parsed = SnapshotEnvelope::parse(&payload)
            .expect("the authority accepts what this poller writes");
        assert_eq!(parsed.schema, "race-schedule/v1");
        assert_eq!(parsed.polled_every_ms, Some(POLLED_EVERY_MS));
        assert_eq!(POLLED_EVERY_MS, 6 * 60 * 60 * 1000, "must match the cron");
    }

    /// The two binaries' seam: what `race-schedule-poll` writes is exactly
    /// what `race-alert-poll` reads back, with no second parser between
    /// them.
    #[test]
    fn a_written_season_reads_back_unchanged() {
        let payload = one_race().envelope().to_string();
        assert_eq!(RaceScheduleBody::from_payload(&payload), Ok(one_race()));
    }

    /// A payload from some other source read as a season would answer
    /// plausibly and wrongly — the alert poller would see no upcoming race
    /// and stay silent through a race weekend. Named, not guessed at.
    #[test]
    fn a_payload_from_another_source_is_refused_by_name() {
        let payload = serde_json::json!({
            "schema": "city-waste/v2",
            "polled_every_ms": 86_400_000,
            "body": {"collected_on": "2026-08-17"},
        })
        .to_string();
        assert_eq!(
            RaceScheduleBody::from_payload(&payload),
            Err(BodyProblem::WrongSchema("city-waste/v2".to_string()))
        );
    }

    /// A broken envelope and a broken body are different facts and must not
    /// share a branch — the first says the write lane is wrong, the second
    /// says this poller's own shape moved.
    #[test]
    fn a_broken_envelope_and_a_broken_body_are_named_apart() {
        assert!(matches!(
            RaceScheduleBody::from_payload("not json"),
            Err(BodyProblem::Envelope(_))
        ));
        let payload = serde_json::json!({
            "schema": "race-schedule/v1",
            "polled_every_ms": POLLED_EVERY_MS,
            "body": {"events": "the whole season, honest"},
        })
        .to_string();
        assert!(matches!(
            RaceScheduleBody::from_payload(&payload),
            Err(BodyProblem::Body(_))
        ));
    }

    /// An off-season snapshot is a legitimate value, not a parse failure:
    /// the body carries a season with no future events and the pane answers
    /// "no races scheduled".
    #[test]
    fn an_empty_season_is_a_legitimate_body() {
        let empty = RaceScheduleBody { events: vec![] };
        let payload = empty.envelope().to_string();
        assert_eq!(RaceScheduleBody::from_payload(&payload), Ok(empty));
    }
}
