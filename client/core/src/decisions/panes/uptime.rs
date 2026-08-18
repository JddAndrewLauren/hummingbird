//! **The uptime question** (#315, ADR-0017 decisions 2/3/4/6), sunk out of
//! `screens/uptime-pane/uptime.ts` by ADR-0025 (#533/M4) — "is the
//! authority, the web origin and the runner answering HTTP right now?"
//!
//! One source, many panes: `server/uptime-probe` writes one
//! `context_snapshots` row per service declared in its committed
//! `services.json`, keyed by the service's own id, all under this one source
//! string. Subjects come from *which keys this source has ever written a
//! row for* — `waste.rs`'s own reasoning, applied here to a source with no
//! per-device binding at all, since nothing on this pane is something a
//! device configures.
//!
//! **The sweeper has no row here at all** (ADR-0017 decision 4): it never
//! opens a listener, so no truthful `url`/`method`/`expect_status` triple
//! exists for it. Its own liveness signal is healthchecks.io, outside this
//! pane.
//!
//! What is here is the *deciding* half: the payload parser that pins the
//! `uptime/v1` body shape (including the wire's own mutual-exclusion
//! invariant between `observed_status` and `error`), the band function, and
//! the stale-escalation rule. What is not here is a single word:
//! [`ProbeGap`] is a structured enum of **kinds**, never a sentence —
//! `uptimeCollapsedHeadline`, `uptimeGlyph` and `ageWords` stay per-client
//! renderings, exactly `waste.rs`'s own split.

use serde::{Deserialize, Serialize};

use super::contract::{AnswerState, Band, PaneAnswerCore};
use super::inputs::{FreshnessFact, PaneEnvelopeFacts, PaneInputs, PaneSnapshotFacts};

/// Both the snapshot's source and the envelope's `schema` — ADR-0017
/// decision 2 enrols this in `server/domain/src/sources.rs` as
/// `Writes::Snapshots`, so no alert ever carries it. Nothing here checks it
/// against that registry (ADR-0015 forbids it); this constant is this
/// module's own and the two agree by review, `waste.rs`'s own note.
pub const SOURCE: &str = "uptime/v1";

/// The one sentinel subject the never-polled state emits — never a real
/// service id, so it can never collide with a genuine key.
pub const NEVER_POLLED_SUBJECT: &str = "pending";

/// ~3h against the probe workflow's hourly cadence (ADR-0017 decision 6):
/// three missed runs is worse here than for any other infra pane, because
/// this lane is the one that would otherwise notice every *other* service
/// going dark.
pub const STALE_AFTER_MS: i64 = 3 * 60 * 60 * 1000;

/// The declared intent for one service — `"on"` or `"off"`, verbatim off the
/// wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Expected {
    On,
    Off,
}

/// The `uptime/v1` payload body — this module's parser is what pins the
/// shape, since the body inside the envelope is deliberately unfrozen and
/// opaque to everything else.
///
/// `observed_status` and `error` are mutually exclusive on the wire
/// (`ProbeBody::from_outcome`, server side) — exactly one is ever set, and
/// [`parse_uptime_body`] refuses a payload claiming both or neither rather
/// than inventing a third reading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeBody {
    pub expected: Expected,
    pub expect_status: i64,
    pub observed_status: Option<i64>,
    pub error: Option<String>,
}

/// Why this pane has no answer — a **kind**, not a sentence.
///
/// **A probe that cannot tell the truth must never render as healthy.**
/// Every arm names what was wrong so a client can say it in its own words,
/// on `WasteGap`'s own discipline; [`ProbeGap::ObservationUnreadable`] is
/// this pane's own addition, for the one shape `waste.rs` has no analogue
/// of — a payload whose two mutually-exclusive fields agree with each
/// other instead of disagreeing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "gap", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ProbeGap {
    /// No snapshot row at all: nothing has ever been fetched for this key.
    NotFetched,
    /// The envelope itself could not be read. `reason` is
    /// `hummingbird_domain::EnvelopeProblem`'s own wording, passed through
    /// as data.
    Malformed { reason: String },
    UnknownSchema { schema: String },
    NotJson,
    NotAnObject,
    /// One or more of `expected`/`expect_status`/`observed_status`/`error`
    /// is missing or the wrong shape.
    FieldsUnreadable,
    /// `observed_status` and `error` were both present or both absent —
    /// the wire never produces this, and reading it as anything else would
    /// invent a third meaning nothing on the server side ever intends.
    ObservationUnreadable,
}

/// Reads one snapshot row into a body, or says why it could not.
pub fn parse_uptime_body(snapshot: Option<&PaneSnapshotFacts>) -> Result<ProbeBody, ProbeGap> {
    let Some(snapshot) = snapshot else {
        return Err(ProbeGap::NotFetched);
    };
    let (schema, body) = match &snapshot.envelope {
        PaneEnvelopeFacts::Malformed { reason } => {
            return Err(ProbeGap::Malformed { reason: reason.clone() })
        }
        PaneEnvelopeFacts::Ok { schema, body } => (schema, body),
    };
    if schema != SOURCE {
        return Err(ProbeGap::UnknownSchema { schema: schema.clone() });
    }

    let parsed: serde_json::Value = serde_json::from_str(body).map_err(|_| ProbeGap::NotJson)?;
    let object = parsed.as_object().ok_or(ProbeGap::NotAnObject)?;

    let expected = match object.get("expected").and_then(|v| v.as_str()) {
        Some("on") => Some(Expected::On),
        Some("off") => Some(Expected::Off),
        _ => None,
    };
    let expect_status = object.get("expect_status").and_then(|v| v.as_i64());
    // Present-and-null vs. absent are different facts here: the field must
    // exist (as either a number or `null`) for the payload to be readable
    // at all, on `isNullableFiniteNumber(undefined) === false`'s own rule.
    let observed_status: Option<Option<i64>> = match object.get("observed_status") {
        Some(serde_json::Value::Null) => Some(None),
        Some(value) => value.as_i64().map(Some),
        None => None,
    };
    let error: Option<Option<String>> = match object.get("error") {
        Some(serde_json::Value::Null) => Some(None),
        Some(serde_json::Value::String(s)) => Some(Some(s.clone())),
        _ => None,
    };

    let (Some(expected), Some(expect_status), Some(observed_status), Some(error)) =
        (expected, expect_status, observed_status, error)
    else {
        return Err(ProbeGap::FieldsUnreadable);
    };
    if observed_status.is_none() == error.is_none() {
        return Err(ProbeGap::ObservationUnreadable);
    }

    Ok(ProbeBody { expected, expect_status, observed_status, error })
}

/// This service's band, at read time, from the raw observation alone —
/// never from a stored judgement.
///
/// **Divergence from the manifest's stated intent is the only thing that
/// lifts a band** (ADR-0017 decision 4): an `expected: off` service
/// correctly unreachable is quiet agreement, `dormant`; an `expected: on`
/// service that cannot be reached at all is the most severe reading,
/// `live`, since the door itself is shut; a reachable-but-wrong-status
/// service is a lesser fault, `near`; and an `expected: off` service that
/// unexpectedly answers is treated as severely as a fully-down
/// `expected: on` service, `live`, since both are the manifest's stated
/// intent being wrong right now.
pub fn uptime_band(body: &ProbeBody) -> Band {
    if body.expected == Expected::Off {
        return if body.error.is_some() { Band::Dormant } else { Band::Live };
    }
    if body.error.is_some() {
        return Band::Live;
    }
    if body.observed_status != Some(body.expect_status) {
        return Band::Near;
    }
    Band::Dormant
}

/// This question's subjects: one per service key this source has ever
/// written a row for, sorted for a stable render order — or the one
/// sentinel subject when nothing has been read yet.
pub fn uptime_subjects(inputs: &PaneInputs) -> Vec<String> {
    let snapshots = inputs.pane_reads.get(SOURCE).map(|read| read.snapshots.as_slice()).unwrap_or(&[]);
    if snapshots.is_empty() {
        return vec![NEVER_POLLED_SUBJECT.to_string()];
    }
    let mut keys: Vec<String> = snapshots.iter().map(|snapshot| snapshot.key.clone()).collect();
    keys.sort();
    keys
}

/// Everything one answered pane needs, decided once — [`WasteFacts`]'s own
/// shape, trimmed to what this pane has.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeFacts {
    pub service_id: String,
    pub body: ProbeBody,
    pub stale: bool,
    pub freshness: FreshnessFact,
}

/// The whole answered fact set, or the reason there is none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProbeResolved {
    Facts(ProbeFacts),
    Gap { gap: ProbeGap },
}

/// Resolves one service's facts, or says why there are none —
/// `resolveProbe` verbatim.
pub fn uptime_facts(service_id: &str, inputs: &PaneInputs) -> ProbeResolved {
    let snapshot = inputs
        .pane_reads
        .get(SOURCE)
        .and_then(|read| read.snapshots.iter().find(|snapshot| snapshot.key == service_id));
    let body = match parse_uptime_body(snapshot) {
        Ok(body) => body,
        Err(gap) => return ProbeResolved::Gap { gap },
    };
    // `parse_uptime_body` returning `Ok` guarantees the row exists.
    let freshness = snapshot.expect("a parsed body implies a snapshot row").freshness;
    ProbeResolved::Facts(ProbeFacts {
        service_id: service_id.to_string(),
        stale: freshness.is_stale_beyond(STALE_AFTER_MS),
        body,
        freshness,
    })
}

/// This question's answer for the shell (#315 over ADR-0017), minus its
/// rendering half.
///
/// **The probe workflow itself going quiet must not read as continued
/// agreement.** Read from the payload alone, a `dormant` band is only as
/// trustworthy as the poller that wrote it — once the hourly workflow
/// stops running, every row it ever wrote keeps reporting whatever it last
/// saw, forever. A stale `dormant` reading is therefore escalated to
/// `imminent` here, `waste.rs`'s own escalation shape but on a single band:
/// unlike `github.rs`'s `dormant`-or-`distant`, `uptime.ts` has no
/// `distant` band at all, so this pane only ever has one dormant reading to
/// escalate. A non-`dormant` reading is left alone — it is already
/// visible, and going stale on top of an already-reported divergence does
/// not need a second escalation.
///
/// `within_band` is always `None`: there is no "next relevant moment" a
/// reachability probe names, the pane's whole story is the present instant.
pub fn uptime_answer(subject_key: &str, inputs: &PaneInputs) -> PaneAnswerCore {
    let gap = PaneAnswerCore {
        answer_state: AnswerState::BoundButUnacquired,
        band: Band::Dormant,
        within_band: None,
    };
    if subject_key == NEVER_POLLED_SUBJECT {
        return gap;
    }
    match uptime_facts(subject_key, inputs) {
        ProbeResolved::Gap { .. } => gap,
        ProbeResolved::Facts(facts) => {
            let band = uptime_band(&facts.body);
            let band = if facts.stale && band == Band::Dormant { Band::Imminent } else { band };
            PaneAnswerCore { answer_state: AnswerState::Answered, band, within_band: None }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_786_636_800_000; // 2026-08-12T16:00:00Z

    fn body(expected: &str, expect_status: i64, observed_status: Option<i64>, error: Option<&str>) -> String {
        serde_json::to_string(&serde_json::json!({
            "expected": expected,
            "expect_status": expect_status,
            "observed_status": observed_status,
            "error": error,
        }))
        .unwrap()
    }

    fn healthy_body() -> String {
        body("on", 401, Some(401), None)
    }

    fn ok_envelope(body: &str) -> serde_json::Value {
        serde_json::json!({"kind":"ok","schema":SOURCE,"body":body})
    }

    fn snapshot_json(key: &str, envelope: serde_json::Value, age_ms: i64) -> serde_json::Value {
        serde_json::json!({
            "key": key,
            "envelope": envelope,
            "freshness": {"kind":"age","ageMs":age_ms,"declaredCadenceMs":3_600_000},
        })
    }

    fn inputs_with_snapshots(snapshots: serde_json::Value) -> PaneInputs {
        serde_json::from_value(serde_json::json!({
            "nowMs": NOW,
            "paneReads": { SOURCE: { "snapshots": snapshots } },
        }))
        .unwrap()
    }

    fn inputs_for(key: &str, body_str: &str) -> PaneInputs {
        inputs_with_snapshots(serde_json::json!([snapshot_json(key, ok_envelope(body_str), 5 * 60_000)]))
    }

    fn no_reads() -> PaneInputs {
        serde_json::from_value(serde_json::json!({"nowMs": NOW})).unwrap()
    }

    // -------------------------------------------------------- parse_uptime_body

    #[test]
    fn reads_a_well_formed_body() {
        let inputs = inputs_for("authority", &healthy_body());
        assert_eq!(
            parse_uptime_body(inputs.pane_reads.get(SOURCE).unwrap().snapshots.first()),
            Ok(ProbeBody { expected: Expected::On, expect_status: 401, observed_status: Some(401), error: None }),
        );
    }

    #[test]
    fn reads_an_unreachable_body_error_present_status_absent() {
        let inputs = inputs_for("runner", &body("on", 401, None, Some("connection refused")));
        assert_eq!(
            parse_uptime_body(inputs.pane_reads.get(SOURCE).unwrap().snapshots.first()),
            Ok(ProbeBody {
                expected: Expected::On,
                expect_status: 401,
                observed_status: None,
                error: Some("connection refused".to_string()),
            }),
        );
    }

    #[test]
    fn names_which_kind_of_gap_it_is_rather_than_answering_healthily() {
        assert_eq!(parse_uptime_body(None), Err(ProbeGap::NotFetched));

        let malformed = inputs_with_snapshots(serde_json::json!([{
            "key": "x",
            "envelope": {"kind": "malformed", "reason": "`body` is missing"},
            "freshness": {"kind": "unknown"},
        }]));
        assert_eq!(
            parse_uptime_body(malformed.pane_reads.get(SOURCE).unwrap().snapshots.first()),
            Err(ProbeGap::Malformed { reason: "`body` is missing".to_string() }),
        );

        let unknown_schema = inputs_with_snapshots(serde_json::json!([{
            "key": "x",
            "envelope": {"kind": "ok", "schema": "uptime/v9", "body": healthy_body()},
            "freshness": {"kind": "unknown"},
        }]));
        assert_eq!(
            parse_uptime_body(unknown_schema.pane_reads.get(SOURCE).unwrap().snapshots.first()),
            Err(ProbeGap::UnknownSchema { schema: "uptime/v9".to_string() }),
        );

        let not_json = inputs_for("x", "{");
        assert_eq!(
            parse_uptime_body(not_json.pane_reads.get(SOURCE).unwrap().snapshots.first()),
            Err(ProbeGap::NotJson),
        );

        let not_an_object = inputs_for("x", "[]");
        assert_eq!(
            parse_uptime_body(not_an_object.pane_reads.get(SOURCE).unwrap().snapshots.first()),
            Err(ProbeGap::NotAnObject),
        );

        let bad_fields = inputs_for(
            "x",
            &serde_json::json!({"expected":"maybe","expect_status":401,"observed_status":401,"error":null})
                .to_string(),
        );
        assert_eq!(
            parse_uptime_body(bad_fields.pane_reads.get(SOURCE).unwrap().snapshots.first()),
            Err(ProbeGap::FieldsUnreadable),
        );

        // Both an observed_status AND an error — the wire never produces
        // this, and reading it as anything but a gap would invent a third
        // reading nothing on the server side ever means.
        let both_set = inputs_for("x", &body("on", 401, Some(401), Some("also this")));
        assert_eq!(
            parse_uptime_body(both_set.pane_reads.get(SOURCE).unwrap().snapshots.first()),
            Err(ProbeGap::ObservationUnreadable),
        );
    }

    // -------------------------------------------------------------- uptime_band

    #[test]
    fn expected_on_reached_the_declared_status_is_dormant_agreement() {
        assert_eq!(
            uptime_band(&ProbeBody { expected: Expected::On, expect_status: 401, observed_status: Some(401), error: None }),
            Band::Dormant,
        );
    }

    #[test]
    fn expected_on_unreachable_is_live_the_door_itself_is_shut() {
        assert_eq!(
            uptime_band(&ProbeBody {
                expected: Expected::On,
                expect_status: 401,
                observed_status: None,
                error: Some("timed out".to_string()),
            }),
            Band::Live,
        );
    }

    #[test]
    fn expected_on_wrong_status_is_near_the_door_is_open_but_not_as_declared() {
        assert_eq!(
            uptime_band(&ProbeBody { expected: Expected::On, expect_status: 401, observed_status: Some(500), error: None }),
            Band::Near,
        );
    }

    #[test]
    fn expected_off_unreachable_is_dormant_agreement_quiet_not_a_status_word() {
        assert_eq!(
            uptime_band(&ProbeBody {
                expected: Expected::Off,
                expect_status: 200,
                observed_status: None,
                error: Some("connection refused".to_string()),
            }),
            Band::Dormant,
        );
    }

    #[test]
    fn expected_off_reachable_is_live_something_is_running_that_should_not_be() {
        assert_eq!(
            uptime_band(&ProbeBody { expected: Expected::Off, expect_status: 200, observed_status: Some(200), error: None }),
            Band::Live,
        );
    }

    // --------------------------------------------------------- uptime_subjects

    #[test]
    fn is_the_never_polled_sentinel_when_nothing_has_been_read() {
        assert_eq!(uptime_subjects(&no_reads()), vec![NEVER_POLLED_SUBJECT.to_string()]);
    }

    #[test]
    fn is_the_never_polled_sentinel_when_the_source_has_read_but_written_nothing() {
        let inputs = inputs_with_snapshots(serde_json::json!([]));
        assert_eq!(uptime_subjects(&inputs), vec![NEVER_POLLED_SUBJECT.to_string()]);
    }

    #[test]
    fn is_every_service_key_this_source_has_ever_written_sorted() {
        let inputs = inputs_with_snapshots(serde_json::json!([
            snapshot_json("web", ok_envelope(&healthy_body()), 0),
            snapshot_json("authority", ok_envelope(&healthy_body()), 0),
            snapshot_json("runner", ok_envelope(&healthy_body()), 0),
        ]));
        assert_eq!(uptime_subjects(&inputs), vec!["authority", "runner", "web"]);
    }

    // ----------------------------------------------------------- uptime_answer

    #[test]
    fn is_a_gap_never_an_unbound_question_when_nothing_has_polled_yet() {
        let answer = uptime_answer(NEVER_POLLED_SUBJECT, &no_reads());
        assert_eq!(answer.answer_state, AnswerState::BoundButUnacquired);
        assert_eq!(answer.band, Band::Dormant);
        assert_eq!(answer.within_band, None);
    }

    #[test]
    fn answers_dormant_agreement_for_a_healthy_expected_on_service() {
        let inputs = inputs_for("authority", &healthy_body());
        let answer = uptime_answer("authority", &inputs);
        assert_eq!(answer.answer_state, AnswerState::Answered);
        assert_eq!(answer.band, Band::Dormant);
    }

    #[test]
    fn answers_dormant_agreement_for_an_expected_off_service_correctly_unreachable() {
        let inputs = inputs_for("runner", &body("off", 401, None, Some("connection refused")));
        let answer = uptime_answer("runner", &inputs);
        assert_eq!(answer.answer_state, AnswerState::Answered);
        assert_eq!(answer.band, Band::Dormant);
    }

    #[test]
    fn answers_live_with_a_distinct_reading_for_an_unreachable_host() {
        let inputs = inputs_for("authority", &body("on", 401, None, Some("connection refused")));
        let answer = uptime_answer("authority", &inputs);
        assert_eq!(answer.band, Band::Live);
    }

    #[test]
    fn answers_live_for_the_fault_the_other_way_something_reachable_that_should_be_off() {
        let inputs = inputs_for("runner", &body("off", 401, Some(401), None));
        let answer = uptime_answer("runner", &inputs);
        assert_eq!(answer.band, Band::Live);
    }

    #[test]
    fn answers_near_for_a_wrong_status_expected_on_service() {
        let inputs = inputs_for("authority", &body("on", 401, Some(500), None));
        let answer = uptime_answer("authority", &inputs);
        assert_eq!(answer.band, Band::Near);
    }

    #[test]
    fn answers_with_a_gap_not_a_healthy_reading_for_a_malformed_payload() {
        let inputs = inputs_for("authority", "not json");
        let answer = uptime_answer("authority", &inputs);
        assert_eq!(answer.band, Band::Dormant);
        assert_eq!(answer.answer_state, AnswerState::BoundButUnacquired);
    }

    #[test]
    fn escalates_a_stale_dormant_reading_rather_than_trusting_a_maybe_dead_poller() {
        let inputs = inputs_with_snapshots(serde_json::json!([snapshot_json(
            "authority",
            ok_envelope(&healthy_body()),
            4 * 60 * 60 * 1000,
        )]));
        let answer = uptime_answer("authority", &inputs);
        assert_eq!(answer.answer_state, AnswerState::Answered);
        assert_eq!(answer.band, Band::Imminent);
    }

    #[test]
    fn does_not_escalate_an_already_visible_divergent_reading_merely_for_being_stale() {
        let inputs = inputs_with_snapshots(serde_json::json!([snapshot_json(
            "authority",
            ok_envelope(&body("on", 401, None, Some("connection refused"))),
            4 * 60 * 60 * 1000,
        )]));
        let answer = uptime_answer("authority", &inputs);
        assert_eq!(answer.band, Band::Live);
    }

    // ------------------------------------------------------------ uptime_facts

    #[test]
    fn bands_stale_at_the_threshold_against_the_hourly_cadence() {
        let fresh = inputs_with_snapshots(serde_json::json!([snapshot_json(
            "authority",
            ok_envelope(&healthy_body()),
            2 * 60 * 60 * 1000,
        )]));
        let stale = inputs_with_snapshots(serde_json::json!([snapshot_json(
            "authority",
            ok_envelope(&healthy_body()),
            4 * 60 * 60 * 1000,
        )]));
        let ProbeResolved::Facts(fresh_facts) = uptime_facts("authority", &fresh) else {
            panic!("expected facts");
        };
        let ProbeResolved::Facts(stale_facts) = uptime_facts("authority", &stale) else {
            panic!("expected facts");
        };
        assert!(!fresh_facts.stale);
        assert!(stale_facts.stale);
    }

    #[test]
    fn never_reads_unknown_freshness_as_fresh() {
        let inputs = inputs_with_snapshots(serde_json::json!([{
            "key": "authority",
            "envelope": ok_envelope(&healthy_body()),
            "freshness": {"kind": "unknown"},
        }]));
        let ProbeResolved::Facts(facts) = uptime_facts("authority", &inputs) else {
            panic!("expected facts");
        };
        assert!(facts.stale);
    }

    #[test]
    fn a_gap_for_a_service_id_this_source_has_never_written() {
        let inputs = inputs_for("authority", &healthy_body());
        assert_eq!(uptime_facts("runner", &inputs), ProbeResolved::Gap { gap: ProbeGap::NotFetched });
    }
}
