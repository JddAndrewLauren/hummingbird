//! **The cross-language contract, and the only guard against it drifting.**
//!
//! The body inside ADR-0015's envelope is deliberately unfrozen and opaque to
//! the server: `SnapshotEnvelope` carries it through as text, `POST
//! /api/snapshots` never looks inside it, and the pane's own parser is what
//! pins its shape. That is the right design — and it means **no type, no
//! schema and no compiler on either side can see the other**. Rename
//! `collected_on` here and every test in both languages still passes while
//! the pane silently reads "no collection schedule has been fetched yet"
//! forever.
//!
//! So this file asserts the literal snake_case key names twice: once on the
//! JSON this crate actually produces, and once against the text of the code
//! that consumes it. The second half is the one that catches a rename made
//! on either side alone.
//!
//! **Retargeted at #534.** ADR-0025/#533 sank the pane's parser out of
//! `client/web/src/screens/waste-pane/waste.ts` and into
//! `client/core/src/decisions/panes/waste.rs::parse_waste_body` — the real
//! parse surface now lives there, and this file's text-greps had gone stale
//! against the TS file's own words (`waste.ts` still exists, but only as a
//! thin rendering wrapper that no longer spells `body.zone` or
//! `value === "trash"` anywhere). Retargeting at the Rust parser is what
//! makes this file a contract test again rather than a permanently-red one.

use hummingbird_city_waste::body::WasteBody;
use hummingbird_city_waste::cadence::Cadence;
use hummingbird_city_waste::date::Date;

/// Every key the pane reads out of the body, paired with the exact
/// substring `parse_waste_body` reads it through — `zone`/`streams` go
/// through a bare `object.get("…")`, `scheduled`/`collected_on` through the
/// shared `day("…")` closure. `cadence` is not here: it is this poller's
/// own addition, the pane ignores it (no unknown-field rejection), and
/// pinning it against the core would assert a coupling that does not exist.
const KEYS_THE_PANE_READS: &[(&str, &str)] = &[
    ("zone", "object.get(\"zone\")"),
    ("scheduled", "day(\"scheduled\")"),
    ("collected_on", "day(\"collected_on\")"),
    ("streams", "object.get(\"streams\")"),
];

const WASTE_RS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../client/core/src/decisions/panes/waste.rs"
));

fn sample() -> WasteBody {
    WasteBody::new(
        "America/Los_Angeles",
        Cadence { anchor: Date::parse("2026-08-03").unwrap(), every_n_weeks: 1 },
        Date::parse("2026-08-18").unwrap(),
        vec!["trash".into(), "recycling".into()],
    )
}

#[test]
fn the_body_this_poller_writes_is_the_body_the_pane_parses() {
    let payload = sample().envelope();
    let body = payload.get("body").expect("the envelope carries a body");
    let object = body.as_object().expect("the body is an object");

    for (key, read_through) in KEYS_THE_PANE_READS {
        assert!(
            object.contains_key(*key),
            "`{key}` is gone from the body this poller writes — `parse_waste_body` \
             would answer a gap and the pane would read 'not fetched yet' forever"
        );
        assert!(
            WASTE_RS.contains(read_through),
            "`{read_through}` is gone from waste.rs's parser — `{key}` is written \
             here but no longer read there, one side was renamed alone"
        );
    }

    // Values, not just names: `scheduled` is derived and `collected_on` is
    // observed, and a body that swapped them would still have both keys.
    assert_eq!(object["scheduled"], "2026-08-17");
    assert_eq!(object["collected_on"], "2026-08-18");
    assert_eq!(object["zone"], "America/Los_Angeles");
    assert_eq!(object["streams"], serde_json::json!(["trash", "recycling"]));
}

/// The pane's stream vocabulary is closed (`Stream::parse`), so a bin name
/// this poller invents would fail its whole parse — `WasteGap::UnknownStream`
/// — rather than being ignored. Checked against `Stream::as_str`'s own match
/// arms, the one place each name is spelled as the wire's literal string.
#[test]
fn every_stream_name_this_poller_can_write_is_one_the_pane_knows() {
    for stream in ["trash", "recycling", "yard"] {
        assert!(
            WASTE_RS.contains(&format!("=> \"{stream}\",")),
            "`{stream}` is not in waste.rs's closed stream vocabulary (`Stream::as_str`)"
        );
    }
}

/// The two constants that have to agree across the seam, checked against the
/// core's own literals: the source string (which is also the envelope's
/// `schema`, and the pane refuses anything else) and the snapshot key (which
/// is also the alert's `subject_key`).
#[test]
fn the_source_and_snapshot_key_agree_with_the_pane() {
    assert!(
        WASTE_RS.contains(r#"pub const SOURCE: &str = "city-waste/v2";"#),
        "the pane reads a different source than this poller writes"
    );
    assert!(
        WASTE_RS.contains(r#"pub const SNAPSHOT_KEY: &str = "collection";"#),
        "the pane reads a different snapshot key than this poller writes"
    );
    assert_eq!(hummingbird_domain::CITY_WASTE_V2, "city-waste/v2");
    assert_eq!(hummingbird_city_waste::body::SNAPSHOT_KEY, "collection");
}

/// The alert half of the wire, checked as **JSON** rather than as a struct.
/// `restamp_on_change` carries a `skip_serializing_if`, so a wrong predicate
/// would drop it silently and every daily re-poll would stop asking the
/// server to decide the stamp — the dismissal-undoing bug, reintroduced by a
/// serde attribute and invisible to any assertion on the struct.
#[test]
fn the_alert_this_poller_posts_carries_restamp_on_change_on_the_wire() {
    use hummingbird_city_waste::alert::plan;
    use hummingbird_city_waste::judge::judge;

    let cadence = Cadence { anchor: Date::parse("2026-08-03").unwrap(), every_n_weeks: 1 };
    let today = Date::parse("2026-08-12").unwrap();
    let collected_on = Date::parse("2026-08-18").unwrap();
    let ingest = plan(cadence, judge(cadence, collected_on, today))
        .expect("a slide rings")
        .ingest("America/Los_Angeles")
        .expect("a real zone");

    let wire: serde_json::Value = serde_json::to_value(&ingest).unwrap();
    assert_eq!(wire["restamp_on_change"], serde_json::json!(true));
    assert_eq!(wire["source"], "city-waste/v2");
    assert_eq!(wire["source_key"], "2026-08-17");
    assert_eq!(wire["subject_key"], "collection");
    assert!(wire.get("raised_at").is_none(), "sending both is a 400");
}
