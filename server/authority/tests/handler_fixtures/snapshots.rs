//! `POST /api/snapshots` — the server-polled context lane's write side
//! (#120). Version-blind upsert on `(source, key)`, ingest scope, the row
//! replaced wholesale by each poll.

use hummingbird_domain::{ChangesResponse, ContextSnapshot};

use crate::rig::*;

/// A well-formed `city-waste/v2` poll body. `fetched_at` and the two dates
/// are parameters because most of the properties below are about what
/// changes between two polls.
fn poll(scheduled: &str, collected_on: &str, fetched_at: i64) -> String {
    format!(
        r#"{{"source": "city-waste/v2", "key": "collection", "fetched_at": {fetched_at},
            "payload": {{"schema": "city-waste/v2", "polled_every_ms": 86400000,
                         "body": {{"zone": "America/Los_Angeles", "scheduled": "{scheduled}",
                                   "collected_on": "{collected_on}",
                                   "streams": ["trash"]}}}}}}"#
    )
}

#[test]
fn first_write_201_and_bumps() {
    let sql = RusqliteSql::new();
    let resp = ingest_snapshot(&sql, &poll("2026-08-17", "2026-08-17", 1000), 5000);
    assert_eq!(resp.status, 201, "{}", resp.body);
    let snapshot: ContextSnapshot = body_as(&resp);
    assert_eq!(snapshot.source, "city-waste/v2");
    assert_eq!(snapshot.key, "collection");
    assert_eq!(
        snapshot.fetched_at, 1000,
        "the poller's read time, never the server's write clock"
    );
    assert_eq!(snapshot.version, 1);
    assert_eq!(meta_version(&sql), 1);
}

#[test]
fn a_changed_payload_replaces_the_row_wholesale() {
    let sql = RusqliteSql::new();
    ingest_snapshot(&sql, &poll("2026-08-17", "2026-08-17", 1000), 0);
    let resp = ingest_snapshot(&sql, &poll("2026-08-17", "2026-08-18", 2000), 0);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let snapshot: ContextSnapshot = body_as(&resp);
    assert!(
        snapshot.payload.contains("2026-08-18"),
        "the new reading replaced the old: {}",
        snapshot.payload
    );
    assert!(
        !snapshot.payload.contains(r#""collected_on":"2026-08-17""#),
        "replaced wholesale, never merged: {}",
        snapshot.payload
    );
    assert_eq!(snapshot.version, 2);
    assert_eq!(meta_version(&sql), 2);

    let rows = sql.exec("SELECT * FROM context_snapshots", &[]).unwrap();
    assert_eq!(rows.len(), 1, "one row per (source, key), never a second");
}

/// The byte-identical re-POST: a retry, a double-fired cron. It carries no
/// new reading at all, so it must not bump the workspace version and push a
/// meaningless delta to every device.
#[test]
fn a_byte_identical_replay_is_no_write_and_no_bump() {
    let sql = RusqliteSql::new();
    let body = poll("2026-08-17", "2026-08-17", 1000);
    ingest_snapshot(&sql, &body, 0);
    let before = meta_version(&sql);

    let resp = ingest_snapshot(&sql, &body, 9999);
    assert_eq!(resp.status, 200, "{}", resp.body);
    assert_eq!(meta_version(&sql), before, "a replay carries no new reading");
    let snapshot: ContextSnapshot = body_as(&resp);
    assert_eq!(snapshot.version, before);
}

/// The counterpart, and the subtle half of the rule: **`fetched_at` is part
/// of the value**. Tomorrow's poll reading the same unchanged answer still
/// writes, because the stamp is what `Freshness::of_snapshot` and each
/// pane's staleness threshold read. Freezing it would take the waste pane
/// stale on day two of a perfectly healthy week, and make "poller fine,
/// nothing changed" indistinguishable from "poller dead" — which is the
/// exact discrimination this lane exists to make.
#[test]
fn a_fresh_poll_of_an_unchanged_answer_still_moves_fetched_at() {
    let sql = RusqliteSql::new();
    ingest_snapshot(&sql, &poll("2026-08-17", "2026-08-17", 1000), 0);
    let before = meta_version(&sql);

    let resp = ingest_snapshot(&sql, &poll("2026-08-17", "2026-08-17", 87_400_000), 0);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let snapshot: ContextSnapshot = body_as(&resp);
    assert_eq!(snapshot.fetched_at, 87_400_000, "the stamp moved");
    assert!(
        meta_version(&sql) > before,
        "and the move reached the delta — a device that never sees it \
         would read a healthy poller as a dead one"
    );
}

/// A retried run finishing after the next one, or two runners overlapping.
/// Answered 200 with the stored row rather than a 409: the poller has
/// nothing to rebase and nothing to do about it.
#[test]
fn an_older_fetched_at_never_overwrites() {
    let sql = RusqliteSql::new();
    ingest_snapshot(&sql, &poll("2026-08-17", "2026-08-18", 5000), 0);
    let before = meta_version(&sql);

    let resp = ingest_snapshot(&sql, &poll("2026-08-17", "2026-08-17", 4000), 0);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let snapshot: ContextSnapshot = body_as(&resp);
    assert_eq!(snapshot.fetched_at, 5000, "the newer stamp stands");
    assert!(
        snapshot.payload.contains("2026-08-18"),
        "and so does the newer reading: {}",
        snapshot.payload
    );
    assert_eq!(meta_version(&sql), before, "no write, no bump");
}

/// ADR-0015's "visibly broken, never quietly empty", enforced at the write
/// rather than at every reader: a payload no pane can parse is a source
/// silently answering nothing, and the poller gets a 400 naming what was
/// wrong instead. Every `EnvelopeProblem` arm, and the reason text is
/// asserted — a 400 that does not say what broke is the quiet emptiness one
/// layer up.
#[test]
fn every_broken_envelope_is_a_400_naming_what_was_wrong() {
    let sql = RusqliteSql::new();
    let cases = [
        (r#""not an object at all""#, "payload is not a JSON object"),
        ("[1, 2, 3]", "payload is not a JSON object"),
        (r#"{"body": {}}"#, "`schema` is missing"),
        (r#"{"schema": null, "body": {}}"#, "`schema` is missing"),
        (r#"{"schema": 7, "body": {}}"#, "`schema` must be a string"),
        (r#"{"schema": "s/v1"}"#, "`body` is missing"),
        (r#"{"schema": "s/v1", "body": null}"#, "`body` is missing"),
        (
            r#"{"schema": "s/v1", "polled_every_ms": 0, "body": {}}"#,
            "`polled_every_ms` must be a positive",
        ),
        (
            r#"{"schema": "s/v1", "polled_every_ms": "6h", "body": {}}"#,
            "`polled_every_ms` must be a positive",
        ),
    ];
    for (payload, expected_reason) in cases {
        let body = format!(
            r#"{{"source": "city-waste/v2", "key": "collection", "fetched_at": 1,
                "payload": {payload}}}"#
        );
        let resp = ingest_snapshot(&sql, &body, 0);
        assert_eq!(resp.status, 400, "{payload}: {}", resp.body);
        assert!(
            resp.body.contains(expected_reason),
            "{payload} → {}, expected it to name {expected_reason:?}",
            resp.body
        );
    }
    assert_eq!(meta_version(&sql), 0, "nothing broken was stored");
}

/// The two checks this route deliberately does **not** make. ADR-0015
/// forbids resolving a snapshot's `schema` against `REGISTRY` — a schema
/// this build has not heard of is a fact about the build — and `schema`
/// need not equal `source`, since one names the payload shape and the other
/// names the feed.
#[test]
fn an_unrecognised_schema_is_stored_untouched() {
    let sql = RusqliteSql::new();
    let resp = ingest_snapshot(
        &sql,
        r#"{"source": "city-waste/v2", "key": "collection", "fetched_at": 1,
            "payload": {"schema": "not-a-real-source/v9", "body": {"anything": true}}}"#,
        0,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    let snapshot: ContextSnapshot = body_as(&resp);
    assert!(
        snapshot.payload.contains("not-a-real-source/v9"),
        "carried through untouched: {}",
        snapshot.payload
    );
}

#[test]
fn empty_identity_and_a_non_positive_fetched_at_are_400s() {
    let sql = RusqliteSql::new();
    let env = r#"{"schema": "s/v1", "body": {}}"#;
    for (case, body) in [
        (
            "empty source",
            format!(r#"{{"source": "", "key": "collection", "fetched_at": 1, "payload": {env}}}"#),
        ),
        (
            "empty key",
            format!(
                r#"{{"source": "city-waste/v2", "key": "", "fetched_at": 1, "payload": {env}}}"#
            ),
        ),
        (
            "zero fetched_at",
            format!(
                r#"{{"source": "city-waste/v2", "key": "collection", "fetched_at": 0,
                     "payload": {env}}}"#
            ),
        ),
        (
            "negative fetched_at",
            format!(
                r#"{{"source": "city-waste/v2", "key": "collection", "fetched_at": -1,
                     "payload": {env}}}"#
            ),
        ),
    ] {
        let resp = ingest_snapshot(&sql, &body, 0);
        assert_eq!(resp.status, 400, "{case}: {}", resp.body);
    }
    // `fetched_at` has no default: omitting it is a parse failure, which is
    // what makes an exact replay decidable at all.
    let resp = ingest_snapshot(
        &sql,
        r#"{"source": "city-waste/v2", "key": "collection", "payload": {"schema": "s/v1", "body": {}}}"#,
        0,
    );
    assert_eq!(resp.status, 400, "an absent fetched_at: {}", resp.body);
}

/// The scope matrix and the source binding, on the new route. A device
/// token is 403 — which is also why `smoke-prod.sh` cannot cover this lane.
#[test]
fn only_a_matching_ingest_token_may_write() {
    let sql = RusqliteSql::new();
    let body = poll("2026-08-17", "2026-08-17", 1000);

    let anon = req_anon(&sql, "POST", "/api/snapshots", None, Some(&body));
    assert_eq!(anon.status, 401, "no credential");
    assert!(anon.body.is_empty(), "401 bodies are empty");

    let device = req(&sql, "POST", "/api/snapshots", None, Some(&body), 0);
    assert_eq!(device.status, 403, "a device may not write a snapshot");
    assert!(device.body.is_empty(), "403 bodies are empty");

    let sweeper = req_as(&sql, SWEEPER_TOKEN, "POST", "/api/snapshots", None, Some(&body), 0);
    assert_eq!(sweeper.status, 403, "the sweeper reaches POST /api/items alone");

    // A correctly-scoped ingest token bound to some *other* source: 403,
    // empty-bodied, exactly as on the alert lane.
    bind_ingest_token(&sql, "healthchecks/v1");
    let wrong_source =
        req_as(&sql, INGEST_TOKEN, "POST", "/api/snapshots", None, Some(&body), 0);
    assert_eq!(wrong_source.status, 403, "{}", wrong_source.body);
    assert!(wrong_source.body.is_empty(), "403 bodies are empty");

    assert_eq!(meta_version(&sql), 0, "nothing refused was written");
}

/// The written row reaches devices through the ordinary lanes, and the
/// delta and the sweep agree on it byte-for-byte — #114's criterion, now
/// with a row this crate actually wrote rather than one seeded raw.
#[test]
fn a_written_snapshot_appears_in_changes_and_sweep_byte_identically() {
    let sql = RusqliteSql::new();
    ingest_snapshot(&sql, &poll("2026-08-17", "2026-08-18", 1000), 0);

    let delta: ChangesResponse = body_as(&changes(&sql, "since=0"));
    assert_eq!(delta.context_snapshots.len(), 1);
    assert_eq!(delta.context_snapshots[0].key, "collection");

    let sweep_resp = sweep(&sql);
    assert_eq!(changes(&sql, "since=0").body, sweep_resp.body, "one code path");

    // And a cursor past the write sees nothing — the gate holds.
    let after: ChangesResponse = body_as(&changes(&sql, &format!("since={}", delta.version)));
    assert!(after.context_snapshots.is_empty());
}

#[test]
fn wrong_method_on_the_collection_is_405() {
    let sql = RusqliteSql::new();
    // GET is no longer wrong here (#135-137) — see `only_a_bound_ingest_token_may_read`
    // and `a_device_token_reads_any_source` below.
    for method in ["PATCH", "DELETE"] {
        let resp = req(&sql, method, "/api/snapshots", None, Some("{}"), 0);
        assert_eq!(resp.status, 405, "{method} /api/snapshots: {}", resp.body);
    }
}

// --------------------------------------------------------- GET (#135-137)

/// The read half of the cursor's durable home: an evaluated-stream poller
/// writes its cursor through the existing `POST /api/snapshots` (city-waste's
/// own lane, generic since #120), then reads it back on the next run through
/// this route. `Device | Ingest`, source-bound for `Ingest` exactly like the
/// write side.
#[test]
fn a_written_snapshot_is_readable_back_by_its_bound_ingest_token() {
    let sql = RusqliteSql::new();
    ingest_snapshot(&sql, &poll("2026-08-17", "2026-08-17", 1000), 0);
    // `ingest_snapshot` rebinds the rig's ingest token to `city-waste/v2`.
    let resp = req_as(
        &sql,
        INGEST_TOKEN,
        "GET",
        "/api/snapshots",
        Some("source=city-waste/v2&key=collection"),
        None,
        0,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let snapshot: ContextSnapshot = body_as(&resp);
    assert_eq!(snapshot.source, "city-waste/v2");
    assert_eq!(snapshot.key, "collection");
}

#[test]
fn a_device_token_reads_any_source() {
    let sql = RusqliteSql::new();
    ingest_snapshot(&sql, &poll("2026-08-17", "2026-08-17", 1000), 0);
    let resp = req(
        &sql,
        "GET",
        "/api/snapshots",
        Some("source=city-waste/v2&key=collection"),
        None,
        0,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
}

#[test]
fn no_such_row_is_404_not_a_gap_with_a_body() {
    let sql = RusqliteSql::new();
    bind_ingest_token(&sql, "gmail/v1");
    let resp = req_as(
        &sql,
        INGEST_TOKEN,
        "GET",
        "/api/snapshots",
        Some("source=gmail/v1&key=cursor"),
        None,
        0,
    );
    assert_eq!(resp.status, 404, "{}", resp.body);
}

#[test]
fn missing_or_empty_query_params_are_400() {
    let sql = RusqliteSql::new();
    for query in [None, Some("source=gmail/v1"), Some("key=cursor"), Some("source=&key=cursor")] {
        let resp = req(&sql, "GET", "/api/snapshots", query, None, 0);
        assert_eq!(resp.status, 400, "{query:?}: {}", resp.body);
    }
}

#[test]
fn an_ingest_token_bound_to_another_source_is_403() {
    let sql = RusqliteSql::new();
    ingest_snapshot(&sql, &poll("2026-08-17", "2026-08-17", 1000), 0);
    bind_ingest_token(&sql, "gmail/v1");
    let resp = req_as(
        &sql,
        INGEST_TOKEN,
        "GET",
        "/api/snapshots",
        Some("source=city-waste/v2&key=collection"),
        None,
        0,
    );
    assert_eq!(resp.status, 403, "{}", resp.body);
    assert!(resp.body.is_empty(), "403 bodies are empty");
}

#[test]
fn an_unauthenticated_read_is_401() {
    let sql = RusqliteSql::new();
    let resp = req_anon(
        &sql,
        "GET",
        "/api/snapshots",
        Some("source=gmail/v1&key=cursor"),
        None,
    );
    assert_eq!(resp.status, 401);
    assert!(resp.body.is_empty());
}
