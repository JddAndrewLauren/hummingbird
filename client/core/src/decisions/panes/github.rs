//! **The GitHub workflow-health question** (#314, ADR-0017 decision 2),
//! sunk out of `screens/github-pane/github.ts` by ADR-0025 (#533/M4, #534).
//!
//! One source, many panes: `server/github-status` writes one
//! `context_snapshots` row per scheduled workflow it finds in this repo's
//! own `.github/workflows/`, keyed by the workflow's file name, all under
//! one source string. This question's [`github_subjects`] is the first
//! sunk pane whose subject list comes from *which keys this source has
//! ever written a row for*, rather than from a per-device binding
//! (`waste.rs`'s single bound page) or a fixed sentinel (`kimi.ts`'s single
//! `"balance"` key).
//!
//! There is no per-device setup here, on `kimi.ts`'s own reasoning: the
//! credential lives in the poller's own Actions secret, never a `settings`
//! row a device configures. So this question has no `unbound` state — it is
//! either never-polled at all ([`NEVER_POLLED_SUBJECT`], one sentinel
//! subject) or answered, one pane per workflow key this source has ever
//! written.
//!
//! **The band is computed here, at read time, from the raw verdict the
//! poller wrote — never from a precomputed "stalled" flag.** [`WorkflowGap`]
//! is a structured enum, not a sentence, on [`super::waste::WasteGap`]'s own
//! discipline: `github.ts`'s gap *reasons* ("No answer has been fetched
//! yet.", "This device doesn't know how to read … yet. Update the app.",
//! …) are renderings and stay in TS; only the *kind* sinks. Likewise
//! `githubCollapsedHeadline`/`githubGlyph`/`ageWords` are not ported — they
//! are per-client rendering built from these decided facts.

use serde::{Deserialize, Serialize};

use super::contract::{AnswerState, Band, PaneAnswerCore};
use super::inputs::{FreshnessFact, PaneEnvelopeFacts, PaneInputs, PaneSnapshotFacts};

/// Both the snapshot's source and the envelope's `schema` — `github.ts`'s
/// own constant, verbatim. Nothing here checks it against the frozen source
/// registry (ADR-0015 forbids that); the two agree by review.
pub const SOURCE: &str = "github-hummingbird/v1";

/// The one sentinel subject the never-polled state emits — never a real
/// workflow file name (every real one ends `.yml`/`.yaml`), so it can never
/// collide with a genuine key.
pub const NEVER_POLLED_SUBJECT: &str = "pending";

/// ~30h against `github-status.yml`'s own daily cadence — `github.ts`'s own
/// value and its own reasoning: this pane's whole purpose is catching a dead
/// cron quickly, so it cannot wait as long as `waste.rs`'s 26h.
pub const STALE_AFTER_MS: i64 = 30 * 60 * 60 * 1000;

/// A scheduled run is judged overdue once its last success is this many
/// multiples of its own declared cadence old — `3×`, not `waste.rs`'s
/// plain multiple, because GitHub Actions' own cron carries more slack.
pub const OVERDUE_MULTIPLIER: i64 = 3;

/// The `github-hummingbird/v1` payload body — this module's parser is what
/// pins the shape. [`parse_workflow_body`] reads the raw `snake_case`
/// payload by hand (never through this struct's own `Deserialize`, on
/// `waste.rs`'s `WasteBody`/`parse_waste_body` precedent), so this
/// struct's derive is only ever exercised on its way *out* — to a client,
/// as `camelCase`, exactly `KimiBody`'s and `ProbeBody`'s own convention.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowBody {
    pub display_name: String,
    pub declared_cadence_ms: Option<i64>,
    pub last_run_conclusion: Option<String>,
    pub last_run_event: Option<String>,
    pub last_run_at_ms: Option<i64>,
    pub last_scheduled_success_at_ms: Option<i64>,
}

/// Why this pane has no answer — a **kind**, not a sentence. Mirrors
/// `parseWorkflowBody`'s five gap reasons 1:1; the words stay in
/// `github.ts` per this module's header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "gap", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WorkflowGap {
    /// No snapshot row at all: nothing has ever been fetched.
    NotFetched,
    /// The envelope itself could not be read. `reason` is
    /// `hummingbird_domain::EnvelopeProblem`'s own wording, passed through
    /// as data — `waste.rs`'s own `Malformed` arm.
    Malformed { reason: String },
    UnknownSchema { schema: String },
    NotJson,
    NotAnObject,
    /// The object parsed, but one of its fields is missing or the wrong
    /// shape — `github.ts`'s "The workflow payload's fields can't be read."
    UnreadableFields,
}

/// Reads one snapshot row into a body, or says why it could not — shape
/// only, `parseWorkflowBody` ported verbatim.
pub fn parse_workflow_body(snapshot: Option<&PaneSnapshotFacts>) -> Result<WorkflowBody, WorkflowGap> {
    let Some(snapshot) = snapshot else {
        return Err(WorkflowGap::NotFetched);
    };
    let (schema, body) = match &snapshot.envelope {
        PaneEnvelopeFacts::Malformed { reason } => {
            return Err(WorkflowGap::Malformed { reason: reason.clone() })
        }
        PaneEnvelopeFacts::Ok { schema, body } => (schema, body),
    };
    if schema != SOURCE {
        return Err(WorkflowGap::UnknownSchema { schema: schema.clone() });
    }

    let parsed: serde_json::Value = serde_json::from_str(body).map_err(|_| WorkflowGap::NotJson)?;
    let object = parsed.as_object().ok_or(WorkflowGap::NotAnObject)?;

    let display_name = match object.get("display_name") {
        Some(serde_json::Value::String(s)) => s.clone(),
        _ => return Err(WorkflowGap::UnreadableFields),
    };
    let declared_cadence_ms =
        nullable_i64(object.get("declared_cadence_ms")).ok_or(WorkflowGap::UnreadableFields)?;
    let last_run_conclusion =
        nullable_string(object.get("last_run_conclusion")).ok_or(WorkflowGap::UnreadableFields)?;
    let last_run_event =
        nullable_string(object.get("last_run_event")).ok_or(WorkflowGap::UnreadableFields)?;
    let last_run_at_ms = nullable_i64(object.get("last_run_at_ms")).ok_or(WorkflowGap::UnreadableFields)?;
    let last_scheduled_success_at_ms =
        nullable_i64(object.get("last_scheduled_success_at_ms")).ok_or(WorkflowGap::UnreadableFields)?;

    Ok(WorkflowBody {
        display_name,
        declared_cadence_ms,
        last_run_conclusion,
        last_run_event,
        last_run_at_ms,
        last_scheduled_success_at_ms,
    })
}

/// `isNullableFiniteNumber` ported: a present `null` is a valid absence
/// (`Some(None)`), a present finite number is a valid value (`Some(Some(_))`),
/// and a missing key or anything else is unreadable (`None`).
fn nullable_i64(value: Option<&serde_json::Value>) -> Option<Option<i64>> {
    match value {
        Some(serde_json::Value::Null) => Some(None),
        Some(v) => v.as_i64().map(Some),
        None => None,
    }
}

/// `isNullableString` ported, on [`nullable_i64`]'s own shape.
fn nullable_string(value: Option<&serde_json::Value>) -> Option<Option<String>> {
    match value {
        Some(serde_json::Value::Null) => Some(None),
        Some(serde_json::Value::String(s)) => Some(Some(s.clone())),
        _ => None,
    }
}

/// Every conclusion GitHub's own API reports that is NOT a success —
/// anything else (including a still-running run's `null`) is not read as a
/// failure at all, since "no conclusion yet" is not "failed".
const FAILURE_CONCLUSIONS: [&str; 5] =
    ["failure", "cancelled", "timed_out", "action_required", "startup_failure"];

/// This workflow's band, at read time, from the raw verdict alone —
/// `githubBand` ported verbatim, including its ordering: never-run
/// (`live`) outranks overdue-against-cadence (`imminent`), which outranks a
/// single failed run (`near`), which outranks an unreadable cadence
/// (`distant`, never `dormant` — see `github.ts`'s own note on why a
/// judgement that could not run must not read as a judgement that came back
/// clean), which outranks healthy (`dormant`).
pub fn github_band(body: &WorkflowBody, now_ms: i64) -> Band {
    if body.last_run_at_ms.is_none() {
        return Band::Live;
    }
    let Some(last_scheduled_success_at_ms) = body.last_scheduled_success_at_ms else {
        // Has run, but never a *scheduled* success in this window — a
        // manual run must never mask this.
        return Band::Imminent;
    };
    if let Some(declared_cadence_ms) = body.declared_cadence_ms {
        let overdue_after_ms = declared_cadence_ms * OVERDUE_MULTIPLIER;
        if now_ms - last_scheduled_success_at_ms > overdue_after_ms {
            return Band::Imminent;
        }
    }
    if let Some(conclusion) = &body.last_run_conclusion {
        if FAILURE_CONCLUSIONS.contains(&conclusion.as_str()) {
            return Band::Near;
        }
    }
    if body.declared_cadence_ms.is_none() {
        // The overdue comparison above never ran — this is a gap in the
        // judgement, not a clean result of it.
        return Band::Distant;
    }
    Band::Dormant
}

/// This question's subjects: one per workflow key this source has ever
/// written a row for, sorted for a stable render order independent of the
/// server's own response order — or the one sentinel subject when nothing
/// has been read yet or nothing has ever been polled. `githubSubjects`
/// ported verbatim.
pub fn github_subjects(inputs: &PaneInputs) -> Vec<String> {
    let Some(read) = inputs.pane_reads.get(SOURCE) else {
        return vec![NEVER_POLLED_SUBJECT.to_string()];
    };
    if read.snapshots.is_empty() {
        return vec![NEVER_POLLED_SUBJECT.to_string()];
    }
    let mut keys: Vec<String> = read.snapshots.iter().map(|snapshot| snapshot.key.clone()).collect();
    keys.sort();
    keys
}

/// Everything one answered pane needs, decided once — `WorkflowView` minus
/// the raw `PaneSnapshotDTO` and the subject key, neither of which any
/// decision reads (the caller already has the subject key it passed in).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowView {
    pub body: WorkflowBody,
    pub stale: bool,
    pub freshness: FreshnessFact,
}

/// The whole answered fact set, or the reason there is none —
/// `resolveWorkflow` ported, minus its textual `reason` (the kind is
/// [`WorkflowGap`]; the words stay in `github.ts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WorkflowResolved {
    View(WorkflowView),
    Gap { gap: WorkflowGap },
}

/// The whole answered fact set for one workflow subject, or the reason
/// there is none — `resolveWorkflow` ported verbatim (minus the sentinel
/// arm, which [`github_answer`] handles itself exactly as `githubAnswer`
/// does).
pub fn github_facts(subject_key: &str, inputs: &PaneInputs) -> WorkflowResolved {
    let read = inputs.pane_reads.get(SOURCE);
    let snapshot = read.and_then(|read| read.snapshots.iter().find(|candidate| candidate.key == subject_key));
    let body = match parse_workflow_body(snapshot) {
        Ok(body) => body,
        Err(gap) => return WorkflowResolved::Gap { gap },
    };
    // `parse_workflow_body` returning `Ok` guarantees the row exists.
    let snapshot = snapshot.expect("a parsed body implies a snapshot row");
    WorkflowResolved::View(WorkflowView {
        body,
        stale: snapshot.freshness.is_stale_beyond(STALE_AFTER_MS),
        freshness: snapshot.freshness,
    })
}

/// This question's answer for the shell (#314 over #245/ADR-0017), minus
/// its rendering half — `githubAnswer`'s decision half ported verbatim.
///
/// `within_band` is always `None`: there is no "next relevant moment" a
/// workflow's own run history names — the pane's whole story is about the
/// past, never a future instant to sort by.
///
/// **The stale escalation happens here, after the base band, exactly where
/// `githubAnswer` does it** — never inside [`github_band`] itself. A
/// `dormant`/`distant` band read from a payload is only as trustworthy as
/// the poller that wrote it: once that poller has itself gone quiet, every
/// row it ever wrote keeps reporting whatever it last saw, forever. So a
/// stale `dormant`/`distant` reading is escalated to `imminent`; a
/// `live`/`imminent`/`near` reading is left alone, since it is already
/// non-green.
pub fn github_answer(subject_key: &str, inputs: &PaneInputs) -> PaneAnswerCore {
    let gap = PaneAnswerCore { answer_state: AnswerState::BoundButUnacquired, band: Band::Dormant, within_band: None };
    if subject_key == NEVER_POLLED_SUBJECT {
        return gap;
    }

    let view = match github_facts(subject_key, inputs) {
        WorkflowResolved::Gap { .. } => return gap,
        WorkflowResolved::View(view) => view,
    };

    let band = github_band(&view.body, inputs.now_ms);
    let band = if view.stale && matches!(band, Band::Dormant | Band::Distant) { Band::Imminent } else { band };
    PaneAnswerCore { answer_state: AnswerState::Answered, band, within_band: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_786_636_800_000; // 2026-08-12T16:00:00Z (github.test.ts's own NOW)
    const FILE_NAME: &str = "gmail-poll.yml";

    #[derive(Default)]
    struct BodyOverrides {
        display_name: Option<&'static str>,
        declared_cadence_ms: Option<Option<i64>>,
        last_run_conclusion: Option<Option<&'static str>>,
        last_run_event: Option<Option<&'static str>>,
        last_run_at_ms: Option<Option<i64>>,
        last_scheduled_success_at_ms: Option<Option<i64>>,
    }

    fn body_json(overrides: BodyOverrides) -> String {
        let display_name = overrides.display_name.unwrap_or("gmail-poll");
        let declared_cadence_ms = overrides.declared_cadence_ms.unwrap_or(Some(15 * 60 * 1000));
        let last_run_conclusion = overrides.last_run_conclusion.unwrap_or(Some("success"));
        let last_run_event = overrides.last_run_event.unwrap_or(Some("schedule"));
        let last_run_at_ms = overrides.last_run_at_ms.unwrap_or(Some(NOW - 60_000));
        let last_scheduled_success_at_ms = overrides.last_scheduled_success_at_ms.unwrap_or(Some(NOW - 60_000));
        serde_json::json!({
            "display_name": display_name,
            "declared_cadence_ms": declared_cadence_ms,
            "last_run_conclusion": last_run_conclusion,
            "last_run_event": last_run_event,
            "last_run_at_ms": last_run_at_ms,
            "last_scheduled_success_at_ms": last_scheduled_success_at_ms,
        })
        .to_string()
    }

    fn ok_envelope(body: &str) -> serde_json::Value {
        serde_json::json!({"kind":"ok","schema":SOURCE,"body":body})
    }

    fn snapshot_json(key: &str, envelope: serde_json::Value, freshness: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"key": key, "envelope": envelope, "freshness": freshness})
    }

    fn fresh() -> serde_json::Value {
        serde_json::json!({"kind":"age","ageMs":60000,"declaredCadenceMs":86400000})
    }

    fn stale() -> serde_json::Value {
        serde_json::json!({"kind":"age","ageMs": STALE_AFTER_MS + 60_000,"declaredCadenceMs":86400000})
    }

    fn inputs_with(snapshots: Vec<serde_json::Value>) -> PaneInputs {
        serde_json::from_value(serde_json::json!({
            "nowMs": NOW,
            "paneReads": { SOURCE: { "snapshots": snapshots } },
        }))
        .unwrap()
    }

    fn inputs_default() -> PaneInputs {
        inputs_with(vec![snapshot_json(FILE_NAME, ok_envelope(&body_json(BodyOverrides::default())), fresh())])
    }

    fn healthy_body() -> WorkflowBody {
        WorkflowBody {
            display_name: "gmail-poll".to_string(),
            declared_cadence_ms: Some(15 * 60 * 1000),
            last_run_conclusion: Some("success".to_string()),
            last_run_event: Some("schedule".to_string()),
            last_run_at_ms: Some(NOW - 60_000),
            last_scheduled_success_at_ms: Some(NOW - 60_000),
        }
    }

    // ---------------------------------------------------------- parse_workflow_body

    #[test]
    fn reads_a_well_formed_body() {
        let inputs = inputs_default();
        assert_eq!(
            parse_workflow_body(inputs.snapshot(SOURCE, FILE_NAME)),
            Ok(healthy_body()),
        );
    }

    #[test]
    fn reads_the_auto_disable_shape_every_field_null_but_the_name() {
        let inputs = inputs_with(vec![snapshot_json(
            FILE_NAME,
            ok_envelope(&body_json(BodyOverrides {
                declared_cadence_ms: Some(None),
                last_run_conclusion: Some(None),
                last_run_event: Some(None),
                last_run_at_ms: Some(None),
                last_scheduled_success_at_ms: Some(None),
                ..Default::default()
            })),
            fresh(),
        )]);
        let parsed = parse_workflow_body(inputs.snapshot(SOURCE, FILE_NAME)).unwrap();
        assert_eq!(parsed.last_run_at_ms, None);
        assert_eq!(parsed.last_scheduled_success_at_ms, None);
    }

    #[test]
    fn names_which_kind_of_gap_it_is_rather_than_answering_emptily() {
        let cases: Vec<(serde_json::Value, WorkflowGap)> = vec![
            (
                serde_json::json!({"kind":"malformed","reason":"`body` is missing"}),
                WorkflowGap::Malformed { reason: "`body` is missing".into() },
            ),
            (ok_envelope("{"), WorkflowGap::NotJson),
            (ok_envelope("[]"), WorkflowGap::NotAnObject),
            (
                ok_envelope(&serde_json::json!({"display_name": 12}).to_string()),
                WorkflowGap::UnreadableFields,
            ),
        ];
        for (envelope, expected) in cases {
            let inputs = inputs_with(vec![snapshot_json(FILE_NAME, envelope, fresh())]);
            assert_eq!(
                parse_workflow_body(inputs.snapshot(SOURCE, FILE_NAME)),
                Err(expected.clone()),
                "{expected:?}",
            );
        }
    }

    #[test]
    fn an_absent_snapshot_is_not_fetched_rather_than_malformed() {
        let inputs = inputs_with(vec![]);
        assert_eq!(parse_workflow_body(inputs.snapshot(SOURCE, FILE_NAME)), Err(WorkflowGap::NotFetched));
    }

    #[test]
    fn reads_an_unrecognised_schema_as_this_device_being_behind() {
        let inputs = inputs_with(vec![snapshot_json(
            FILE_NAME,
            serde_json::json!({"kind":"ok","schema":"github-hummingbird/v9","body": body_json(BodyOverrides::default())}),
            fresh(),
        )]);
        assert_eq!(
            parse_workflow_body(inputs.snapshot(SOURCE, FILE_NAME)),
            Err(WorkflowGap::UnknownSchema { schema: "github-hummingbird/v9".to_string() }),
        );
    }

    // -------------------------------------------------------------- github_band

    #[test]
    fn bands_a_healthy_on_cadence_workflow_dormant() {
        assert_eq!(github_band(&healthy_body(), NOW), Band::Dormant);
    }

    #[test]
    fn bands_the_never_run_auto_disable_shape_live_the_most_severe_reading() {
        let body = WorkflowBody { last_run_at_ms: None, last_scheduled_success_at_ms: None, ..healthy_body() };
        assert_eq!(github_band(&body, NOW), Band::Live);
    }

    #[test]
    fn bands_a_manual_only_run_imminent_a_green_manual_run_never_masks_a_dead_cron() {
        let body = WorkflowBody {
            last_run_event: Some("workflow_dispatch".to_string()),
            last_scheduled_success_at_ms: None,
            ..healthy_body()
        };
        assert_eq!(github_band(&body, NOW), Band::Imminent);
    }

    #[test]
    fn bands_overdue_against_the_declared_cadence_imminent_even_with_a_stored_success() {
        let cadence = 15 * 60 * 1000;
        let just_under_threshold = NOW - cadence * OVERDUE_MULTIPLIER + 1;
        let just_over_threshold = NOW - cadence * OVERDUE_MULTIPLIER - 1;
        let under = WorkflowBody {
            declared_cadence_ms: Some(cadence),
            last_scheduled_success_at_ms: Some(just_under_threshold),
            ..healthy_body()
        };
        let over = WorkflowBody {
            declared_cadence_ms: Some(cadence),
            last_scheduled_success_at_ms: Some(just_over_threshold),
            ..healthy_body()
        };
        assert_eq!(github_band(&under, NOW), Band::Dormant);
        assert_eq!(github_band(&over, NOW), Band::Imminent);
    }

    #[test]
    fn bands_a_single_recent_failure_near_less_urgent_than_a_stopped_cron() {
        let body = WorkflowBody { last_run_conclusion: Some("failure".to_string()), ..healthy_body() };
        assert_eq!(github_band(&body, NOW), Band::Near);
    }

    #[test]
    fn a_stopped_cron_sorts_above_a_single_failure_the_briefs_own_acceptance_line() {
        use super::super::contract::BAND_ORDER;
        let stopped = WorkflowBody { last_run_at_ms: None, last_scheduled_success_at_ms: None, ..healthy_body() };
        let failed = WorkflowBody { last_run_conclusion: Some("failure".to_string()), ..healthy_body() };
        let stopped_band = github_band(&stopped, NOW);
        let failed_band = github_band(&failed, NOW);
        let index = |band: Band| BAND_ORDER.iter().position(|b| *b == band).unwrap();
        assert!(index(stopped_band) < index(failed_band));
    }

    #[test]
    fn does_not_judge_overdue_ness_when_the_cadence_is_unrecognised_and_never_reports_that_as_healthy() {
        let body = WorkflowBody {
            declared_cadence_ms: None,
            last_scheduled_success_at_ms: Some(NOW - 999_999_999),
            ..healthy_body()
        };
        let band = github_band(&body, NOW);
        assert_eq!(band, Band::Distant);
        assert_ne!(band, Band::Dormant);
    }

    #[test]
    fn still_reports_a_genuine_failure_near_ahead_of_an_unreadable_cadence_distant() {
        let body = WorkflowBody {
            declared_cadence_ms: None,
            last_run_conclusion: Some("failure".to_string()),
            ..healthy_body()
        };
        assert_eq!(github_band(&body, NOW), Band::Near);
    }

    // ----------------------------------------------------------- github_subjects

    #[test]
    fn returns_the_sentinel_when_nothing_has_been_read_yet() {
        let inputs: PaneInputs = serde_json::from_value(serde_json::json!({"nowMs": NOW})).unwrap();
        assert_eq!(github_subjects(&inputs), vec![NEVER_POLLED_SUBJECT.to_string()]);
    }

    #[test]
    fn returns_the_sentinel_when_the_source_has_been_read_but_has_no_rows() {
        let inputs = inputs_with(vec![]);
        assert_eq!(github_subjects(&inputs), vec![NEVER_POLLED_SUBJECT.to_string()]);
    }

    #[test]
    fn returns_one_subject_per_snapshot_key_sorted() {
        let inputs = inputs_with(vec![
            snapshot_json("graph-mail-poll.yml", ok_envelope(&body_json(BodyOverrides::default())), fresh()),
            snapshot_json("calendar-poll.yml", ok_envelope(&body_json(BodyOverrides::default())), fresh()),
        ]);
        assert_eq!(
            github_subjects(&inputs),
            vec!["calendar-poll.yml".to_string(), "graph-mail-poll.yml".to_string()],
        );
    }

    // ------------------------------------------------------------- github_answer

    #[test]
    fn answers_with_the_workflows_own_band_when_a_snapshot_has_landed() {
        let inputs = inputs_default();
        let answer = github_answer(FILE_NAME, &inputs);
        assert_eq!(answer.answer_state, AnswerState::Answered);
        assert_eq!(answer.band, Band::Dormant);
        assert_eq!(answer.within_band, None);
    }

    #[test]
    fn is_a_gap_never_an_unbound_question_for_the_never_polled_sentinel() {
        let inputs = inputs_with(vec![]);
        let answer = github_answer(NEVER_POLLED_SUBJECT, &inputs);
        assert_eq!(answer.answer_state, AnswerState::BoundButUnacquired);
        assert_eq!(answer.band, Band::Dormant);
    }

    #[test]
    fn bands_live_once_a_workflow_has_never_run_at_all() {
        let inputs = inputs_with(vec![snapshot_json(
            FILE_NAME,
            ok_envelope(&body_json(BodyOverrides {
                last_run_at_ms: Some(None),
                last_scheduled_success_at_ms: Some(None),
                last_run_conclusion: Some(None),
                last_run_event: Some(None),
                ..Default::default()
            })),
            fresh(),
        )]);
        assert_eq!(github_answer(FILE_NAME, &inputs).band, Band::Live);
    }

    #[test]
    fn escalates_a_stale_dormant_healthy_reading_to_imminent() {
        let inputs = inputs_with(vec![snapshot_json(
            FILE_NAME,
            ok_envelope(&body_json(BodyOverrides::default())),
            stale(),
        )]);
        let answer = github_answer(FILE_NAME, &inputs);
        assert_ne!(answer.band, Band::Dormant);
        assert_eq!(answer.band, Band::Imminent);
    }

    #[test]
    fn escalates_a_stale_distant_cadence_unreadable_reading_to_imminent() {
        let inputs = inputs_with(vec![snapshot_json(
            FILE_NAME,
            ok_envelope(&body_json(BodyOverrides { declared_cadence_ms: Some(None), ..Default::default() })),
            stale(),
        )]);
        assert_eq!(github_answer(FILE_NAME, &inputs).band, Band::Imminent);
    }

    #[test]
    fn does_not_need_a_second_escalation_for_a_band_that_is_already_non_green_when_stale() {
        let inputs = inputs_with(vec![snapshot_json(
            FILE_NAME,
            ok_envelope(&body_json(BodyOverrides {
                last_run_conclusion: Some(Some("failure")),
                ..Default::default()
            })),
            stale(),
        )]);
        assert_eq!(github_answer(FILE_NAME, &inputs).band, Band::Near);
    }

    #[test]
    fn never_reads_unknown_freshness_as_fresh_against_this_panes_own_threshold() {
        let inputs = inputs_with(vec![snapshot_json(
            FILE_NAME,
            ok_envelope(&body_json(BodyOverrides::default())),
            serde_json::json!({"kind":"unknown"}),
        )]);
        // An unknown-age answer is never fresh, so a dormant reading built
        // on it must escalate exactly as a stale one does.
        assert_eq!(github_answer(FILE_NAME, &inputs).band, Band::Imminent);
    }

    #[test]
    fn bands_stale_at_thirty_hours_against_the_daily_cadence() {
        let fresh_row = inputs_with(vec![snapshot_json(
            FILE_NAME,
            ok_envelope(&body_json(BodyOverrides::default())),
            serde_json::json!({"kind":"age","ageMs": 29 * 60 * 60 * 1000,"declaredCadenceMs":86400000}),
        )]);
        let stale_row = inputs_with(vec![snapshot_json(
            FILE_NAME,
            ok_envelope(&body_json(BodyOverrides::default())),
            serde_json::json!({"kind":"age","ageMs": 31 * 60 * 60 * 1000,"declaredCadenceMs":86400000}),
        )]);
        let fresh_view = match github_facts(FILE_NAME, &fresh_row) {
            WorkflowResolved::View(view) => view,
            WorkflowResolved::Gap { gap } => panic!("expected a view, got {gap:?}"),
        };
        let stale_view = match github_facts(FILE_NAME, &stale_row) {
            WorkflowResolved::View(view) => view,
            WorkflowResolved::Gap { gap } => panic!("expected a view, got {gap:?}"),
        };
        assert!(!fresh_view.stale);
        assert!(stale_view.stale);
    }
}
