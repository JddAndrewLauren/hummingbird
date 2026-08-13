//! Reading `GET /repos/{owner}/{repo}/actions/workflows/{file_name}/runs`'s
//! response, and deciding this workflow's verdict from it.
//!
//! ```text
//! { "total_count": 3, "workflow_runs": [
//!   { "event": "schedule", "conclusion": "success",
//!     "run_started_at": "2026-08-12T13:40:11Z" },
//!   ...
//! ]}
//! ```
//!
//! **Two independent facts, read off the same list, and neither one derived
//! from the other.** [`decide`]'s [`Verdict::last_run`] is the most recent
//! run of *any* event — what a reader means by "did the last run
//! succeed" — while [`Verdict::last_scheduled_success_at_ms`] is filtered to
//! `event == "schedule"` **and** `conclusion == "success"`, because a manual
//! `workflow_dispatch` run must never mask a dead cron (#314's own scenario:
//! the last run was manual and green, the cron itself has not fired in
//! weeks). Both are computed by scanning for the maximum timestamp rather
//! than trusting the API's own ordering — the API does return newest-first,
//! but nothing here needs that guarantee to hold.

use serde::Deserialize;

use crate::instant::parse_iso8601_utc;

/// One run, as parsed off the wire. Unknown fields are ignored by
/// `serde_json`'s default behaviour — this poller reads three fields off a
/// response that carries dozens.
#[derive(Debug, Deserialize)]
struct RawRun {
    event: String,
    conclusion: Option<String>,
    /// The instant the run actually started — preferred over `created_at`,
    /// which for a queued run can predate the run actually executing.
    /// `None` (both fields absent, or unparsable) means this run cannot be
    /// timestamped and is dropped from consideration — an untimestamped
    /// run cannot be compared against anything.
    run_started_at: Option<String>,
    created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawRunsResponse {
    #[serde(default)]
    workflow_runs: Vec<RawRun>,
}

/// One run this poller can reason about: an event, an optional conclusion
/// (a still-running run has none), and a timestamp already resolved to
/// epoch ms.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowRun {
    pub event: String,
    pub conclusion: Option<String>,
    pub ran_at_ms: i64,
}

/// Why the runs response could not be read.
#[derive(Debug, Clone, PartialEq)]
pub enum RunsError {
    Malformed(String),
}

impl std::fmt::Display for RunsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunsError::Malformed(reason) => write!(f, "malformed workflow-runs response: {reason}"),
        }
    }
}

/// Reads the runs-list response into every run this poller can timestamp.
/// A run whose `run_started_at`/`created_at` is absent or unparsable is
/// silently dropped, never a hard failure — a whole response failing to
/// parse at all is the only thing named as an error here, on the same
/// "fail loudly on the shape, not on one odd row" reasoning `race-poll`'s
/// `schedule.rs` gives for an absent optional session.
pub fn parse_runs(body: &str) -> Result<Vec<WorkflowRun>, RunsError> {
    let response: RawRunsResponse =
        serde_json::from_str(body).map_err(|e| RunsError::Malformed(e.to_string()))?;

    let runs = response
        .workflow_runs
        .into_iter()
        .filter_map(|raw| {
            let stamp = raw.run_started_at.or(raw.created_at)?;
            let ran_at_ms = parse_iso8601_utc(&stamp)?;
            Some(WorkflowRun { event: raw.event, conclusion: raw.conclusion, ran_at_ms })
        })
        .collect();
    Ok(runs)
}

/// This workflow's most recent run, of any triggering event.
#[derive(Debug, Clone, PartialEq)]
pub struct LastRun {
    pub conclusion: Option<String>,
    pub event: String,
    pub ran_at_ms: i64,
}

/// This workflow's whole verdict, read off its run history.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Verdict {
    /// `None` only when the runs list is empty — the auto-disable tell
    /// (#314's own scenario): a disabled workflow has no failing run, it
    /// has *no run at all* in the window this poller asked about.
    pub last_run: Option<LastRun>,
    /// The most recent instant this workflow's cron actually produced a
    /// green run. `None` if it never has — including the case where every
    /// run in the window is a manual `workflow_dispatch`, which must never
    /// stand in for this.
    pub last_scheduled_success_at_ms: Option<i64>,
}

/// Decides one workflow's verdict from its run history, in no particular
/// order — every run is compared by its own `ran_at_ms`, never trusted to
/// arrive newest-first.
pub fn decide(runs: &[WorkflowRun]) -> Verdict {
    let mut verdict = Verdict::default();

    for run in runs {
        if verdict.last_run.as_ref().is_none_or(|last| run.ran_at_ms > last.ran_at_ms) {
            verdict.last_run = Some(LastRun {
                conclusion: run.conclusion.clone(),
                event: run.event.clone(),
                ran_at_ms: run.ran_at_ms,
            });
        }
        if run.event == "schedule" && run.conclusion.as_deref() == Some("success") {
            let is_newer = verdict.last_scheduled_success_at_ms.is_none_or(|m| run.ran_at_ms > m);
            if is_newer {
                verdict.last_scheduled_success_at_ms = Some(run.ran_at_ms);
            }
        }
    }

    verdict
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../tests/fixtures/runs-healthy.json");
    const NO_RUNS: &str = include_str!("../tests/fixtures/runs-empty.json");
    const MANUAL_ONLY: &str = include_str!("../tests/fixtures/runs-manual-only.json");
    const RECENT_FAILURE: &str = include_str!("../tests/fixtures/runs-recent-failure.json");
    const MALFORMED: &str = include_str!("../tests/fixtures/runs-malformed.json");

    #[test]
    fn a_healthy_workflow_reads_its_last_scheduled_success() {
        let runs = parse_runs(OK).expect("the fixture parses");
        let verdict = decide(&runs);
        assert_eq!(verdict.last_run.as_ref().unwrap().conclusion, Some("success".to_string()));
        assert_eq!(verdict.last_run.as_ref().unwrap().event, "schedule");
        assert!(verdict.last_scheduled_success_at_ms.is_some());
    }

    /// The auto-disable shape: an empty runs list reads as "never run",
    /// never as an error.
    #[test]
    fn no_runs_at_all_is_the_auto_disable_shape_not_an_error() {
        let runs = parse_runs(NO_RUNS).expect("an empty list still parses");
        assert!(runs.is_empty());
        let verdict = decide(&runs);
        assert_eq!(verdict.last_run, None);
        assert_eq!(verdict.last_scheduled_success_at_ms, None);
    }

    /// #314's own named scenario: the only recent run was a manual
    /// `workflow_dispatch`, and it must not stand in for a scheduled
    /// success — the cron itself may be dead underneath a green manual run.
    #[test]
    fn a_manual_only_recent_run_does_not_count_as_a_scheduled_success() {
        let runs = parse_runs(MANUAL_ONLY).expect("the fixture parses");
        let verdict = decide(&runs);
        assert_eq!(verdict.last_run.as_ref().unwrap().event, "workflow_dispatch");
        assert_eq!(verdict.last_run.as_ref().unwrap().conclusion, Some("success".to_string()));
        assert_eq!(
            verdict.last_scheduled_success_at_ms, None,
            "no scheduled run has ever succeeded in this window"
        );
    }

    /// A single failed scheduled run, with an earlier scheduled success
    /// still in the window — the "less urgent than a stopped cron" case the
    /// brief names.
    #[test]
    fn a_recent_failure_still_reads_the_earlier_scheduled_success() {
        let runs = parse_runs(RECENT_FAILURE).expect("the fixture parses");
        let verdict = decide(&runs);
        assert_eq!(verdict.last_run.as_ref().unwrap().conclusion, Some("failure".to_string()));
        assert_eq!(verdict.last_run.as_ref().unwrap().event, "schedule");
        assert!(verdict.last_scheduled_success_at_ms.is_some());
        assert!(verdict.last_scheduled_success_at_ms.unwrap() < verdict.last_run.as_ref().unwrap().ran_at_ms);
    }

    #[test]
    fn a_response_with_no_workflow_runs_key_at_all_is_read_as_empty() {
        let runs = parse_runs("{}").expect("a missing key defaults to no runs");
        assert!(runs.is_empty());
    }

    #[test]
    fn a_malformed_response_is_named_rather_than_guessed_at() {
        assert!(matches!(parse_runs(MALFORMED), Err(RunsError::Malformed(_))));
        assert!(matches!(parse_runs("not json"), Err(RunsError::Malformed(_))));
    }

    /// A run with neither `run_started_at` nor `created_at` cannot be
    /// timestamped and is silently dropped rather than failing the whole
    /// response.
    #[test]
    fn an_untimestamped_run_is_dropped_not_a_failure() {
        let body = r#"{"workflow_runs":[{"event":"schedule","conclusion":"success"}]}"#;
        let runs = parse_runs(body).expect("the response as a whole still parses");
        assert!(runs.is_empty());
    }

    /// Decide never trusts input ordering: the earlier scheduled success
    /// listed AFTER the later failed run in the fixture still resolves as
    /// the max.
    #[test]
    fn decide_scans_for_the_maximum_timestamp_rather_than_trusting_order() {
        let runs = vec![
            WorkflowRun { event: "schedule".to_string(), conclusion: Some("success".to_string()), ran_at_ms: 100 },
            WorkflowRun { event: "schedule".to_string(), conclusion: Some("failure".to_string()), ran_at_ms: 300 },
            WorkflowRun { event: "schedule".to_string(), conclusion: Some("success".to_string()), ran_at_ms: 200 },
        ];
        let verdict = decide(&runs);
        assert_eq!(verdict.last_run.unwrap().ran_at_ms, 300);
        assert_eq!(verdict.last_scheduled_success_at_ms, Some(200));
    }
}
