//! **The cross-language contract, and the only guard against it drifting.**
//!
//! The body inside ADR-0015's envelope is deliberately unfrozen and opaque
//! to the server: `SnapshotEnvelope` carries it through as text, `POST
//! /api/snapshots` never looks inside it, and the pane's own parser
//! (`client/web/src/screens/github-pane/github.ts::parseWorkflowBody`) is
//! what pins its shape. So this file asserts the literal snake_case key
//! names twice — once on the JSON this crate actually produces, and once
//! against the text of the TypeScript that consumes it — on
//! `kimi-balance/tests/contract.rs`'s own reasoning: no type, no schema and
//! no compiler on either side can see the other, and a rename made on only
//! one side would otherwise still pass every test in both languages while
//! the pane silently read "no answer yet" forever.

use hummingbird_github_status::body::{WorkflowStatusBody, POLLED_EVERY_MS};

const KEYS_THE_PANE_READS: &[&str] = &[
    "display_name",
    "declared_cadence_ms",
    "last_run_conclusion",
    "last_run_event",
    "last_run_at_ms",
    "last_scheduled_success_at_ms",
];

const GITHUB_TS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../client/web/src/screens/github-pane/github.ts"
));

fn sample() -> WorkflowStatusBody {
    WorkflowStatusBody {
        display_name: "gmail-poll".to_string(),
        declared_cadence_ms: Some(15 * 60 * 1000),
        last_run_conclusion: Some("failure".to_string()),
        last_run_event: Some("schedule".to_string()),
        last_run_at_ms: Some(1_772_942_400_000),
        last_scheduled_success_at_ms: Some(1_772_856_000_000),
    }
}

#[test]
fn the_body_this_poller_writes_is_the_body_the_pane_parses() {
    let payload = sample().envelope();
    let body = payload.get("body").expect("the envelope carries a body");
    let object = body.as_object().expect("the body is an object");

    for key in KEYS_THE_PANE_READS {
        assert!(
            object.contains_key(*key),
            "`{key}` is gone from the body this poller writes — `parseWorkflowBody` \
             would answer a gap and the pane would read 'not fetched yet' forever"
        );
        assert!(
            GITHUB_TS.contains(&format!("body.{key}")),
            "`{key}` is written here but no longer read by github.ts — one side \
             was renamed alone"
        );
    }

    assert_eq!(object["display_name"], "gmail-poll");
    assert_eq!(object["declared_cadence_ms"], 900_000);
    assert_eq!(object["last_run_conclusion"], "failure");
    assert_eq!(object["last_run_event"], "schedule");
    assert_eq!(object["last_run_at_ms"], 1_772_942_400_000_i64);
    assert_eq!(object["last_scheduled_success_at_ms"], 1_772_856_000_000_i64);
}

/// The source string, checked against the TypeScript's own literal — the
/// pane refuses anything else against the envelope's `schema`.
#[test]
fn the_source_agrees_with_the_pane() {
    assert!(
        GITHUB_TS.contains(r#"export const SOURCE = "github-hummingbird/v1""#),
        "the pane reads a different source than this poller writes"
    );
    assert_eq!(hummingbird_domain::GITHUB_HUMMINGBIRD_V1, "github-hummingbird/v1");
}

/// `.github/workflows/github-status.yml`'s cron must agree with this — the
/// declared cadence `Freshness` reads for the poller's own pane staleness.
#[test]
fn polled_every_ms_is_one_day() {
    assert_eq!(POLLED_EVERY_MS, 24 * 60 * 60 * 1000);
}
