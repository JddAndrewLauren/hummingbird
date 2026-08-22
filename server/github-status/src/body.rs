//! The `context_snapshots.payload` this poller writes: ADR-0015's envelope
//! around a `github-hummingbird/v1` body, one row per scheduled workflow.
//!
//! **This is one half of a cross-language contract.** The other half is
//! `client/core/src/decisions/panes/github.rs`'s `parse_workflow_body`
//! (sunk out of `client/web/src/screens/github-pane/github.ts` at #534),
//! and nothing mechanically connects them — the same "no type, no schema,
//! no compiler on either side can see the other" posture `kimi-balance`'s
//! `tests/contract.rs` documents, and this crate's own `tests/contract.rs`
//! asserts the same way.
//!
//! **The verdict, not a precomputed band.** This body carries the raw facts
//! `runs.rs::Verdict` decided — the last run's conclusion/event/age, the
//! last scheduled success's age, and the workflow's own declared cadence —
//! and nothing derived from "now" at all (ADR-0002: an answer is never
//! stored). Whether a workflow currently reads as stalled is the pane's own
//! read-time judgement against `inputs.nowMs`, exactly as every other
//! pane's band is (`kimi.ts`'s `kimiBand`, `race.ts`'s `raceAnswer`) — a
//! "stalled: true" field here would be a second place that judgement could
//! disagree with itself between the moment this poller ran and the moment
//! a reader looks at the pane hours later.

use serde::Serialize;

/// How often this poller says it runs, for `Freshness`'s declared cadence.
/// **Must match `.github/workflows/github-status.yml`'s cron.** Every half
/// hour — the manifest scan and the run-history fetch are both cheap, and a
/// poller cannot resolve anything finer than its own interval.
///
/// **This was daily, argued from the wrong end of the range**: "no reason
/// to poll more often than the workflows it watches run at their *slowest*"
/// (daily, `city-waste.yml`). The pane judges each workflow against its own
/// cadence, so what bounds this interval is the *fastest* thing watched —
/// the `*/15` pollers — and a daily reading left the pane a day behind the
/// lane it exists to watch. The workflow's own header carries the rest.
pub const POLLED_EVERY_MS: i64 = 30 * 60 * 1000;

/// One workflow's whole reported state — the `github-hummingbird/v1` body,
/// written once per scheduled workflow under `context_snapshots.key` =
/// the workflow's file name.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkflowStatusBody {
    /// The workflow's own `name:`, e.g. `"gmail-poll"` — human words for the
    /// pane, since the `context_snapshots.key` (the file name) is not
    /// always the same string.
    pub display_name: String,
    /// The tightest cadence this workflow's own `schedule:` entries declare
    /// (`cron::tightest_cadence_ms`), or `None` if this poller's cron
    /// reader does not recognise the shape — a pane reading `None` here
    /// reports the conclusion but cannot judge overdue-ness against a
    /// cadence nothing here could work out.
    pub declared_cadence_ms: Option<i64>,
    /// The most recent run's conclusion, of any triggering event — `None`
    /// only when there is no run history at all in the window this poller
    /// asked about (the auto-disable shape).
    pub last_run_conclusion: Option<String>,
    /// The most recent run's triggering event (`"schedule"`,
    /// `"workflow_dispatch"`, ...) — carried alongside the conclusion so the
    /// pane can tell a green *manual* run apart from a green *scheduled*
    /// one, which is exactly the distinction that catches a live-looking
    /// dead cron.
    pub last_run_event: Option<String>,
    pub last_run_at_ms: Option<i64>,
    /// The most recent instant a **scheduled** run actually succeeded.
    /// `None` if none ever has in this window — including when every run
    /// seen was manual.
    pub last_scheduled_success_at_ms: Option<i64>,
}

impl WorkflowStatusBody {
    /// Wraps the body in ADR-0015's envelope — the `payload` value of one
    /// `POST /api/snapshots` call.
    pub fn envelope(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": hummingbird_domain::GITHUB_HUMMINGBIRD_V1,
            "polled_every_ms": POLLED_EVERY_MS,
            "body": self,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> WorkflowStatusBody {
        WorkflowStatusBody {
            display_name: "gmail-poll".to_string(),
            declared_cadence_ms: Some(15 * 60 * 1000),
            last_run_conclusion: Some("success".to_string()),
            last_run_event: Some("schedule".to_string()),
            last_run_at_ms: Some(1_772_942_400_000),
            last_scheduled_success_at_ms: Some(1_772_942_400_000),
        }
    }

    /// The envelope half of the contract: `SnapshotEnvelope::parse` is the
    /// exact check `POST /api/snapshots` runs, so anything this poller can
    /// build must survive it.
    #[test]
    fn the_envelope_this_poller_builds_passes_the_authoritys_own_parse() {
        let payload = sample().envelope().to_string();
        let parsed = hummingbird_domain::SnapshotEnvelope::parse(&payload)
            .expect("the authority accepts what this poller writes");
        assert_eq!(parsed.schema, "github-hummingbird/v1");
        assert_eq!(parsed.polled_every_ms, Some(POLLED_EVERY_MS));
    }

    /// The auto-disable shape (no runs at all, no cadence to compare
    /// against) must still build a valid envelope — every field is
    /// `Option`, and `None` is a legitimate value throughout, not a
    /// failure to construct.
    #[test]
    fn a_never_run_workflow_still_builds_a_valid_envelope() {
        let body = WorkflowStatusBody {
            display_name: "graph-mail-poll".to_string(),
            declared_cadence_ms: None,
            last_run_conclusion: None,
            last_run_event: None,
            last_run_at_ms: None,
            last_scheduled_success_at_ms: None,
        };
        let payload = body.envelope().to_string();
        hummingbird_domain::SnapshotEnvelope::parse(&payload)
            .expect("a body of every field None still parses");
    }
}
