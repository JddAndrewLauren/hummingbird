//! **The cross-language contract, and the only guard against it drifting.**
//!
//! The body inside ADR-0015's envelope is deliberately unfrozen and opaque
//! to the server: `SnapshotEnvelope` carries it through as text, `POST
//! /api/snapshots` never looks inside it, and the pane's own parser
//! (`client/web/src/screens/uptime-pane/uptime.ts::parseUptimeBody`) is
//! what pins its shape. So this file asserts the literal snake_case key
//! names twice — once on the JSON this crate actually produces, and once
//! against the text of the TypeScript that consumes it — on
//! `kimi-balance/tests/contract.rs`'s own reasoning: **no type, no schema
//! and no compiler on either side can see the other**, and a rename made on
//! only one side would otherwise still pass every test in both languages
//! while the pane silently read "no answer yet" forever.

use hummingbird_uptime_probe::body::{ProbeBody, POLLED_EVERY_MS};
use hummingbird_uptime_probe::manifest::{parse_manifest, Service, SERVICES_JSON};
use hummingbird_uptime_probe::verdict::Outcome;

/// Every key the pane reads out of the body, spelled exactly as it appears
/// on the wire.
const KEYS_THE_PANE_READS: &[&str] = &["expected", "expect_status", "observed_status", "error"];

const UPTIME_TS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../client/web/src/screens/uptime-pane/uptime.ts"
));

fn authority_service() -> Service {
    parse_manifest(SERVICES_JSON)
        .expect("services.json parses")
        .into_iter()
        .find(|s| s.id == "authority")
        .expect("authority is declared")
}

#[test]
fn the_body_this_poller_writes_is_the_body_the_pane_parses() {
    let body = ProbeBody::from_outcome(&authority_service(), &Outcome::Reached(401));
    let payload = body.envelope();
    let object = payload.get("body").expect("the envelope carries a body").as_object().expect("an object");

    for key in KEYS_THE_PANE_READS {
        assert!(
            object.contains_key(*key),
            "`{key}` is gone from the body this poller writes — `parseUptimeBody` \
             would answer a gap and the pane would read 'no answer yet' forever"
        );
        assert!(
            UPTIME_TS.contains(&format!("body.{key}")),
            "`{key}` is written here but no longer read by uptime.ts — one side was renamed alone"
        );
    }
}

/// The source string, checked against the TypeScript's own literal — the
/// pane refuses anything else against the envelope's `schema`.
#[test]
fn the_source_agrees_with_the_pane() {
    assert!(
        UPTIME_TS.contains(r#"export const SOURCE = "uptime/v1""#),
        "the pane reads a different source than this poller writes"
    );
    assert_eq!(hummingbird_domain::UPTIME_V1, "uptime/v1");
}

/// `.github/workflows/uptime-probe.yml`'s cron must agree with this — the
/// declared cadence `Freshness` reads.
#[test]
fn polled_every_ms_is_one_hour() {
    assert_eq!(POLLED_EVERY_MS, 60 * 60 * 1000);
}
