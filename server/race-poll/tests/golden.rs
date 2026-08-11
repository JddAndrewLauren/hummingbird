//! **The body contract, guarded before its consumer exists.**
//!
//! `server/city-waste`'s `tests/contract.rs` asserts the literal snake_case
//! keys against `waste.ts`'s own text, and its header says why: *nothing
//! mechanical connects the two sides*. The body inside ADR-0015's envelope
//! is deliberately unfrozen and opaque to the server, so a rename on either
//! side compiles and passes on both.
//!
//! Here the other side does not exist yet — #119's pane is a separate slice
//! — so this crate would otherwise ship a body with no consumer and no
//! guard. **`tests/fixtures/golden-body.json` is that guard**: the exact
//! envelope this poller emits from the committed Jolpica response, compared
//! byte for byte. #119 writes its parser against that file and adds its own
//! `contract.rs` pointing at it. Same move as
//! `runner/test/parse-capture.test.js`, which reads the real shipped schema
//! off disk as the counterweight to a faked seam.
//!
//! **Regenerating the golden file to make this test pass is the one thing
//! that defeats it.** The file is a contract with a consumer in another PR;
//! a diff here is a decision about that contract, not a chore.

use hummingbird_race_poll::body::POLLED_EVERY_MS;
use hummingbird_race_poll::schedule;

const JOLPICA: &str = include_str!("fixtures/jolpica-current.json");
const GOLDEN: &str = include_str!("fixtures/golden-body.json");

fn golden_from_the_committed_feed() -> String {
    let season = schedule::parse(JOLPICA).expect("the committed feed parses");
    let mut rendered =
        serde_json::to_string_pretty(&season.envelope()).expect("the envelope serializes");
    rendered.push('\n');
    rendered
}

#[test]
fn the_envelope_this_poller_writes_is_the_golden_body_byte_for_byte() {
    assert_eq!(
        golden_from_the_committed_feed(),
        GOLDEN,
        "the body this poller emits has moved away from \
         tests/fixtures/golden-body.json — which is #119's parser contract, \
         not a snapshot to regenerate. If the change is deliberate, it is a \
         decision to take with that consumer."
    );
}

/// The keys #119 will read, spelled exactly as they appear on the wire, and
/// asserted on the golden file's own text rather than through this crate's
/// serde — the same reason `city-waste`'s contract test does not go through
/// its own types.
#[test]
fn the_golden_body_carries_the_keys_the_pane_will_read() {
    let golden: serde_json::Value = serde_json::from_str(GOLDEN).expect("the golden file is JSON");
    assert_eq!(golden["schema"], "race-schedule/v1");
    assert_eq!(golden["polled_every_ms"], serde_json::json!(POLLED_EVERY_MS));

    let events = golden["body"]["events"]
        .as_array()
        .expect("the body carries an events array");
    let first = events.first().expect("the season is not empty");
    for key in ["name", "locality", "starts_at_ms", "sessions"] {
        assert!(first.get(key).is_some(), "an event carries `{key}`");
    }
    let session = first["sessions"]
        .as_array()
        .and_then(|s| s.first())
        .expect("the first event carries a ladder");
    for key in ["kind", "label", "starts_at_ms"] {
        assert!(session.get(key).is_some(), "a session carries `{key}`");
    }

    // The two shape rules a reader could otherwise get wrong, stated on the
    // committed bytes: the race start is on the event, and the ladder never
    // contains the race.
    assert!(
        first["starts_at_ms"].as_i64().unwrap()
            > first["sessions"].as_array().unwrap().iter()
                .map(|s| s["starts_at_ms"].as_i64().unwrap())
                .max()
                .unwrap(),
        "`sessions` holds only the supporting ladder"
    );
    assert!(
        first.get("ends_at_ms").is_none(),
        "the feed has no end time and this body does not invent one"
    );
    assert!(
        first.get("zone").is_none(),
        "a race start is an instant; ADR-0015 leaves the zone device-local"
    );
}
