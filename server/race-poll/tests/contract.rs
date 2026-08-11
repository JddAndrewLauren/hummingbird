//! **The cross-language contract, and the only guard against it drifting.**
//!
//! `server/city-waste`'s `tests/contract.rs` is the pattern and its header is
//! the argument: the body inside ADR-0015's envelope is deliberately unfrozen
//! and opaque to the server, so **no type, no schema and no compiler on either
//! side can see the other**. Rename `starts_at_ms` here and every test in both
//! languages still passes while the pane reads "no schedule has been fetched
//! for this series yet" forever.
//!
//! `tests/golden.rs` guards the bytes this poller emits; #119's consumer now
//! exists, so this file guards the *other* half — that the pane's own parser
//! still reads the keys those bytes carry. It asserts the literal snake_case
//! names against the text of `race.ts`, plus the two constants that have to
//! agree across the seam and one that has to agree with the cron.

use hummingbird_race_poll::body::POLLED_EVERY_MS;

/// The golden envelope, as committed. Read as text rather than rebuilt from
/// this crate's serde, for `city-waste`'s own reason: a contract test that
/// goes through the producer's types cannot see a rename that moved both.
const GOLDEN: &str = include_str!("fixtures/golden-body.json");

const RACE_TS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../client/web/src/screens/race-pane/race.ts"
));

/// Every key the pane reads out of an event, spelled exactly as it appears on
/// the wire, and the accessor spelling `race.ts` reads it with.
const EVENT_KEYS: &[&str] = &["name", "locality", "starts_at_ms", "sessions"];

/// The same, one level down, on the supporting ladder.
const SESSION_KEYS: &[&str] = &["kind", "label", "starts_at_ms"];

fn golden() -> serde_json::Value {
    serde_json::from_str(GOLDEN).expect("the golden file is JSON")
}

#[test]
fn the_body_this_poller_writes_is_the_body_the_pane_parses() {
    let golden = golden();
    let first = golden["body"]["events"]
        .as_array()
        .and_then(|events| events.first())
        .expect("the golden season is not empty")
        .clone();

    for key in EVENT_KEYS {
        assert!(
            first.get(*key).is_some(),
            "`{key}` is gone from the body this poller writes — `parseRaceBody` \
             would answer a gap and the pane would read 'never polled' forever"
        );
        assert!(
            RACE_TS.contains(&format!("event.{key}")),
            "`{key}` is written here but no longer read by race.ts — one side \
             was renamed alone"
        );
    }

    let session = first["sessions"]
        .as_array()
        .and_then(|sessions| sessions.first())
        .expect("the first event carries a ladder")
        .clone();
    for key in SESSION_KEYS {
        assert!(session.get(*key).is_some(), "a session carries `{key}`");
        assert!(
            RACE_TS.contains(&format!("session.{key}")),
            "`{key}` is written on a session but no longer read by race.ts"
        );
    }
}

/// The two constants that have to agree across the seam, checked against the
/// TypeScript's own literals: the source string (which is also the envelope's
/// `schema`, and the pane refuses anything else) and the binding key both
/// sides read the followed series out of.
#[test]
fn the_source_and_binding_key_agree_with_the_pane() {
    assert!(
        RACE_TS.contains(r#"export const SOURCE = "race-schedule/v1""#),
        "the pane reads a different source than this poller writes"
    );
    assert!(
        RACE_TS.contains(r#"export const BINDING_KEY = "race-series""#),
        "the pane reads a different binding than this poller reads"
    );
    assert_eq!(hummingbird_domain::RACE_SCHEDULE_V1, "race-schedule/v1");
    assert_eq!(hummingbird_race_poll::binding::BINDING_KEY, "race-series");
    assert_eq!(golden()["schema"], "race-schedule/v1");
}

/// The pane's staleness threshold is `2 ×` this poller's declared cadence
/// (ADR-0015's rule, unamended for this lane) — a number that lives on the
/// far side of the seam and would otherwise drift silently if the schedule
/// cron ever changed.
#[test]
fn the_panes_stale_threshold_is_twice_the_cadence_this_poller_declares() {
    assert_eq!(golden()["polled_every_ms"], serde_json::json!(POLLED_EVERY_MS));
    assert_eq!(2 * POLLED_EVERY_MS, 12 * 60 * 60 * 1000);
    assert!(
        RACE_TS.contains("export const STALE_AFTER_MS = 12 * 60 * 60 * 1000"),
        "the pane's stale line no longer sits at 2 x this poller's cadence"
    );
}

/// The one thing the pane must never do, asserted on its text: there is no
/// `ends_at_ms` in this body and no read-time verdict may invent one. #266
/// refused to fabricate a per-kind duration into the stored row, and deciding
/// "under way" from a start alone would only move that invention across the
/// seam — so #119 dropped the under-way headline instead, and the start
/// instant is the pane's horizon, matching this source's own
/// `Expiry::Always("the race's start time")`.
#[test]
fn neither_side_invents_a_session_end_time() {
    let golden = golden();
    let first = &golden["body"]["events"][0];
    assert!(first.get("ends_at_ms").is_none());
    assert!(first["sessions"][0].get("ends_at_ms").is_none());
    assert!(
        !RACE_TS.contains("ends_at_ms") && !RACE_TS.contains("endsAtMs"),
        "race.ts reads or invents a session end time; the feed publishes none"
    );
}
