//! **The cross-language contract, and the only guard against it drifting.**
//!
//! The body inside ADR-0015's envelope is deliberately unfrozen and opaque to
//! the server: `SnapshotEnvelope` carries it through as text, `POST
//! /api/snapshots` never looks inside it, and the pane's own parser
//! (`client/web/src/screens/waste-pane/waste.ts::parseWasteBody`) is what
//! pins its shape. That is the right design — and it means **no type, no
//! schema and no compiler on either side can see the other**. Rename
//! `collected_on` here and every test in both languages still passes while
//! the pane silently reads "no collection schedule has been fetched yet"
//! forever.
//!
//! So this file asserts the literal snake_case key names twice: once on the
//! JSON this crate actually produces, and once against the text of the
//! TypeScript that consumes it. The second half is the one that catches a
//! rename made on either side alone.

use hummingbird_city_waste::body::WasteBody;
use hummingbird_city_waste::cadence::Cadence;
use hummingbird_city_waste::date::Date;

/// Every key the pane reads out of the body, spelled exactly as it appears
/// on the wire. `cadence` is not here: it is this poller's own addition, the
/// pane ignores it (`parseWasteBody` does no unknown-field rejection), and
/// pinning it against the TS would assert a coupling that does not exist.
const KEYS_THE_PANE_READS: &[&str] = &["zone", "scheduled", "collected_on", "streams"];

const WASTE_TS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../client/web/src/screens/waste-pane/waste.ts"
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

    for key in KEYS_THE_PANE_READS {
        assert!(
            object.contains_key(*key),
            "`{key}` is gone from the body this poller writes — `parseWasteBody` \
             would answer a gap and the pane would read 'not fetched yet' forever"
        );
        assert!(
            WASTE_TS.contains(&format!("body.{key}")),
            "`{key}` is written here but no longer read by waste.ts — one side \
             was renamed alone"
        );
    }

    // Values, not just names: `scheduled` is derived and `collected_on` is
    // observed, and a body that swapped them would still have both keys.
    assert_eq!(object["scheduled"], "2026-08-17");
    assert_eq!(object["collected_on"], "2026-08-18");
    assert_eq!(object["zone"], "America/Los_Angeles");
    assert_eq!(object["streams"], serde_json::json!(["trash", "recycling"]));
}

/// The pane's stream vocabulary is closed (`isStream`), so a bin name this
/// poller invents would fail its whole parse — "the collection payload lists
/// an unknown kind of bin" — rather than being ignored.
#[test]
fn every_stream_name_this_poller_can_write_is_one_the_pane_knows() {
    for stream in ["trash", "recycling", "yard"] {
        assert!(
            WASTE_TS.contains(&format!("value === \"{stream}\"")),
            "`{stream}` is not in waste.ts's closed stream vocabulary"
        );
    }
}

/// The two constants that have to agree across the seam, checked against the
/// TypeScript's own literals: the source string (which is also the envelope's
/// `schema`, and the pane refuses anything else) and the snapshot key (which
/// is also the alert's `subject_key`).
#[test]
fn the_source_and_snapshot_key_agree_with_the_pane() {
    assert!(
        WASTE_TS.contains(r#"export const SOURCE = "city-waste/v2""#),
        "the pane reads a different source than this poller writes"
    );
    assert!(
        WASTE_TS.contains(r#"export const SNAPSHOT_KEY = "collection""#),
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
    let ingest = plan(cadence, judge(cadence, collected_on, today), today)
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
