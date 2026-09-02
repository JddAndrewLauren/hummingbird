//! **The cross-language contract, and the only guard against it drifting.**
//!
//! The body inside ADR-0015's envelope is deliberately unfrozen and opaque
//! to the server: `SnapshotEnvelope` carries it through as text, `POST
//! /api/snapshots` never looks inside it, and the pane's own parser is what
//! pins its shape. So this file asserts the literal snake_case key names
//! twice — once on the JSON this crate actually produces, and once against
//! the text of the code that consumes it — on `kimi-balance/tests/
//! contract.rs`'s own reasoning: no type, no schema and no compiler on
//! either side can see the other, and a rename made on only one side would
//! otherwise still pass every test in both languages while the pane
//! silently read "no answer yet" forever.
//!
//! **Retargeted at #534.** ADR-0025/#534 sank the pane's parser out of
//! `client/web/src/screens/github-pane/github.ts` and into
//! `client/core/src/decisions/panes/github.rs::parse_workflow_body` — the
//! real parse surface now lives there (`github.ts` kept its name but is now
//! a thin rendering wrapper over the seam, and no longer spells `body.…`
//! anywhere).

use hummingbird_github_status::body::{WorkflowStatusBody, POLLED_EVERY_MS};

const KEYS_THE_PANE_READS: &[&str] = &[
    "display_name",
    "declared_cadence_ms",
    "last_run_conclusion",
    "last_run_event",
    "last_run_at_ms",
    "last_scheduled_success_at_ms",
];

const GITHUB_RS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../client/core/src/decisions/panes/github.rs"
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
            "`{key}` is gone from the body this poller writes — `parse_workflow_body` \
             would answer a gap and the pane would read 'not fetched yet' forever"
        );
        assert!(
            GITHUB_RS.contains(&format!("object.get(\"{key}\")")),
            "`object.get(\"{key}\")` is gone from github.rs's parser — `{key}` is \
             written here but no longer read there, one side was renamed alone"
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
        GITHUB_RS.contains(r#"pub const SOURCE: &str = "github-hummingbird/v1";"#),
        "the pane reads a different source than this poller writes"
    );
    assert_eq!(hummingbird_domain::GITHUB_HUMMINGBIRD_V1, "github-hummingbird/v1");
}

/// `crontab`'s entry for `github-status-poll` must agree with this — the
/// declared cadence `Freshness` reads for the poller's own pane staleness.
/// **#774 moved this poller off `.github/workflows/github-status.yml`'s
/// Actions `schedule:` onto the sweeper's own `crontab`**, so this is
/// checked against that file now, the same pattern `gmail-poll`'s own
/// `tests/contract.rs` uses (itself following
/// `server/uptime-probe/tests/contract.rs`'s reasoning: a bare
/// `assert_eq!` restates the constant and would still pass the day someone
/// changed the `crontab` entry and left this one alone).
const CRONTAB: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../crontab"));

#[test]
fn polled_every_ms_is_half_an_hour_and_the_crontab_says_so_too() {
    assert_eq!(POLLED_EVERY_MS, 30 * 60 * 1000);

    let line = CRONTAB
        .lines()
        .find(|l| l.contains("/app/bin/github-status-poll"))
        .expect("crontab carries an entry for github-status-poll");
    let minute_field = line.split_whitespace().next().expect("a minute field");
    assert_eq!(
        minute_field.split(',').count(),
        2,
        "github-status-poll's crontab entry no longer fires twice an hour — POLLED_EVERY_MS is \
         now a lie"
    );
    assert!(
        line.contains(r#"HB_INGEST_TOKEN="$GH_STATUS_INGEST_TOKEN""#),
        "github-status-poll's crontab entry no longer maps GH_STATUS_INGEST_TOKEN onto HB_INGEST_TOKEN"
    );
    assert!(
        line.contains(r#"GITHUB_TOKEN="$GH_STATUS_PAT""#),
        "github-status-poll's crontab entry no longer maps GH_STATUS_PAT onto GITHUB_TOKEN — \
         without it this binary has no way to authenticate to the GitHub API outside Actions"
    );
}
