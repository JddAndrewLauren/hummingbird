//! **The body contracts, guarded before their consumers exist.**
//!
//! `race-poll`'s `tests/golden.rs` is the pattern and its header is the
//! argument: the body inside ADR-0015's envelope is deliberately unfrozen
//! and opaque to the server, so nothing mechanical connects this poller to
//! the pane that will parse it (#855) or to Settings' discovered-leagues
//! line. **These two golden files are that connection**: the exact
//! envelopes this crate builds from the committed typed fixtures, compared
//! byte for byte.
//!
//! The inputs are this crate's own typed shapes, not Yahoo's wire format —
//! that parse is #854's, and it will pin itself against a saved sample of
//! the real thing. What is pinned here is the *output*.
//!
//! **Regenerating a golden file to make this test pass is the one thing
//! that defeats it.** A diff here is a decision about a contract with a
//! consumer, not a chore.

use hummingbird_fantasy_poll::body::{DiscoveredBody, LineupBody, DISCOVERED_KEY, POLLED_EVERY_MS};
use hummingbird_fantasy_poll::roster::{Roster, Team};

const ROSTER: &str = include_str!("fixtures/roster.json");
const TEAMS: &str = include_str!("fixtures/teams.json");
const GOLDEN_LINEUP: &str = include_str!("fixtures/golden-lineup.json");
const GOLDEN_DISCOVERED: &str = include_str!("fixtures/golden-discovered.json");

/// The fixture's fetch instant: Tue 2026-09-29 15:00Z, before week 4's
/// Thursday-night kickoff.
const AS_OF_MS: i64 = 1_790_694_000_000;

fn pretty(envelope: serde_json::Value) -> String {
    let mut rendered = serde_json::to_string_pretty(&envelope).expect("the envelope serializes");
    rendered.push('\n');
    rendered
}

#[test]
fn the_lineup_envelope_is_the_golden_body_byte_for_byte() {
    let roster: Roster = serde_json::from_str(ROSTER).expect("the fixture roster parses");
    assert_eq!(
        pretty(LineupBody::from_roster(&roster, AS_OF_MS).envelope()),
        GOLDEN_LINEUP,
        "the lineup body has moved away from tests/fixtures/golden-lineup.json — the \
         pane's parser contract (#855), not a snapshot to regenerate"
    );
}

#[test]
fn the_discovered_leagues_envelope_is_the_golden_body_byte_for_byte() {
    let teams: Vec<Team> = serde_json::from_str(TEAMS).expect("the fixture teams parse");
    assert_eq!(
        pretty(DiscoveredBody::from_teams("449", &teams).envelope()),
        GOLDEN_DISCOVERED,
        "the discovered-leagues body has moved away from \
         tests/fixtures/golden-discovered.json — Settings' contract, not a snapshot to regenerate"
    );
}

/// The keys a consumer will read, asserted on the golden file's own text
/// rather than through this crate's serde — a contract test that goes
/// through the producer's types cannot see a rename that moved both.
#[test]
fn the_golden_lineup_carries_what_the_brief_names() {
    let golden: serde_json::Value = serde_json::from_str(GOLDEN_LINEUP).unwrap();
    assert_eq!(golden["schema"], "yahoo-lineup/v1");
    assert_eq!(golden["polled_every_ms"], serde_json::json!(6 * 60 * 60 * 1000));
    assert_eq!(golden["polled_every_ms"], serde_json::json!(POLLED_EVERY_MS));

    let body = &golden["body"];
    for key in ["team_key", "team_name", "league_key", "league_name", "week", "slots"] {
        assert!(body.get(key).is_some(), "the body carries `{key}`");
    }
    let slots = body["slots"].as_array().expect("slots is an array");
    for slot in slots {
        for key in ["position", "occupant", "invalid"] {
            assert!(slot.get(key).is_some(), "every slot carries `{key}`, null or not");
        }
        if let Some(occupant) = slot["occupant"].as_object() {
            for key in ["player_key", "name", "bye_week", "designation", "kickoff_ms"] {
                assert!(occupant.contains_key(key), "an occupant carries `{key}`");
            }
        }
        if let Some(invalid) = slot["invalid"].as_object() {
            for key in ["reason", "deadline_ms"] {
                assert!(invalid.contains_key(key), "an invalid slot carries `{key}`");
            }
        }
    }

    // The fixture's facts, on the committed bytes: one slot per rule, a
    // questionable starter who is valid, and the earliest deadline on a
    // bench player's Thursday-night game.
    let reasons: Vec<&str> = slots.iter().filter_map(|s| s["invalid"]["reason"].as_str()).collect();
    assert_eq!(reasons, vec!["bye", "empty", "out"]);
    let questionable = slots
        .iter()
        .find(|s| s["occupant"]["designation"] == "questionable")
        .expect("the fixture starts a questionable player");
    assert!(questionable["invalid"].is_null());
    assert!(slots.iter().any(|s| s["invalid"]["deadline_ms"] == serde_json::json!(1_790_900_100_000i64)));
    assert!(body.get("zone").is_none(), "instants only; the zone is device-local");
}

#[test]
fn the_golden_discovered_leagues_carry_the_keys_to_bind() {
    let golden: serde_json::Value = serde_json::from_str(GOLDEN_DISCOVERED).unwrap();
    assert_eq!(golden["schema"], "yahoo-discovered-leagues/v1");
    assert_eq!(golden["polled_every_ms"], serde_json::json!(POLLED_EVERY_MS));
    assert_eq!(golden["body"]["game_key"], "449");
    let leagues = golden["body"]["leagues"].as_array().unwrap();
    assert_eq!(leagues.len(), 2);
    for league in leagues {
        for key in ["league_key", "league_name"] {
            assert!(league.get(key).is_some(), "a discovered league carries `{key}`");
        }
    }
    assert_eq!(DISCOVERED_KEY, "discovered-leagues");
}
