//! `GET /api/changes?since=N` — the S0 delta suite. The all-tables pull,
//! `/api/sweep`, and the byte-for-byte agreement tests join later in #114.

use hummingbird_domain::ChangesResponse;

use crate::rig::*;

#[test]
fn changes_since_current_is_empty_and_reads_only_meta() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    let recording = RecordingSql::new(&sql);
    let resp = changes(&recording, "since=1");
    assert_eq!(resp.status, 200);
    let parsed: ChangesResponse = body_as(&resp);
    assert_eq!(parsed.version, 1);
    assert!(parsed.items.is_empty());
    let statements = recording.statements.borrow();
    assert_eq!(
        statements.len(),
        1,
        "an unchanged workspace costs one statement: {statements:?}"
    );
    assert!(
        statements[0].contains("FROM meta") && !statements[0].contains("items"),
        "and that statement reads meta, not items: {}",
        statements[0]
    );
}

#[test]
fn changes_since_older_returns_only_rows_above_cursor() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a", "title": "first"}"#, 0); // version 1
    post(&sql, r#"{"id": "b", "title": "second"}"#, 0); // version 2
    patch(&sql, "a", r#"{"expected_version": 1, "title": "first-renamed"}"#, 0); // version 3
    let resp = changes(&sql, "since=2");
    let parsed: ChangesResponse = body_as(&resp);
    assert_eq!(parsed.version, 3);
    assert_eq!(parsed.items.len(), 1, "only the re-versioned row");
    assert_eq!(parsed.items[0].id, "a");
    assert_eq!(parsed.items[0].title, "first-renamed");
}

#[test]
fn changes_since_zero_is_the_full_sweep() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a", "title": "first"}"#, 0);
    post(&sql, r#"{"id": "b", "title": "second"}"#, 0);
    let parsed: ChangesResponse = body_as(&changes(&sql, "since=0"));
    assert_eq!(parsed.items.len(), 2);
    assert!(
        parsed.items[0].version < parsed.items[1].version,
        "ordered by version"
    );
}

#[test]
fn changes_since_above_current_version_returns_empty_with_server_version() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a", "title": "first"}"#, 0); // version 1
    let parsed: ChangesResponse = body_as(&changes(&sql, "since=999"));
    assert!(parsed.items.is_empty());
    assert_eq!(parsed.version, 1, "the actual server version, not the cursor");
}

#[test]
fn changes_since_missing_or_non_numeric_400() {
    let sql = RusqliteSql::new();
    for query in ["", "since=abc", "cursor=1"] {
        let resp = changes(&sql, query);
        assert_eq!(resp.status, 400, "query {query:?}: {}", resp.body);
    }
    let no_query = req(&sql, "GET", "/api/changes", None, None, 0);
    assert_eq!(no_query.status, 400);
}
