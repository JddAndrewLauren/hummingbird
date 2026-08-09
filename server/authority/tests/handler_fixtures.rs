//! Native fixture tests for the S0 routes (#113): every acceptance
//! criterion, against real SQLite (rusqlite in memory) behind the same
//! [`Sql`] seam the Durable Object drives. Zero live credentials.

use std::cell::RefCell;

use hummingbird_authority::{
    handle, init_schema, ApiRequest, ApiResponse, Row, Sql, SqlError, SqlValue,
};
use hummingbird_domain::{ChangesResponse, ConflictResponse, Item};

// ------------------------------------------------------------------ rig

struct RusqliteSql {
    conn: rusqlite::Connection,
}

impl RusqliteSql {
    fn new() -> Self {
        let sql = RusqliteSql {
            conn: rusqlite::Connection::open_in_memory().expect("in-memory sqlite opens"),
        };
        init_schema(&sql).expect("schema initializes");
        sql
    }
}

impl Sql for RusqliteSql {
    fn exec(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, SqlError> {
        let mut stmt = self.conn.prepare(sql).map_err(|e| SqlError {
            message: e.to_string(),
        })?;
        let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rusqlite_params: Vec<rusqlite::types::Value> = params
            .iter()
            .map(|p| match p {
                SqlValue::Null => rusqlite::types::Value::Null,
                SqlValue::Integer(n) => rusqlite::types::Value::Integer(*n),
                SqlValue::Real(f) => rusqlite::types::Value::Real(*f),
                SqlValue::Text(s) => rusqlite::types::Value::Text(s.clone()),
            })
            .collect();
        let mut rows_out = Vec::new();
        let mut rows = stmt
            .query(rusqlite::params_from_iter(rusqlite_params))
            .map_err(|e| SqlError {
                message: e.to_string(),
            })?;
        while let Some(row) = rows.next().map_err(|e| SqlError {
            message: e.to_string(),
        })? {
            let mut out = Row::new();
            for (i, name) in names.iter().enumerate() {
                let value = match row.get_ref(i).map_err(|e| SqlError {
                    message: e.to_string(),
                })? {
                    rusqlite::types::ValueRef::Null => SqlValue::Null,
                    rusqlite::types::ValueRef::Integer(n) => SqlValue::Integer(n),
                    rusqlite::types::ValueRef::Real(f) => SqlValue::Real(f),
                    rusqlite::types::ValueRef::Text(t) => {
                        SqlValue::Text(String::from_utf8_lossy(t).into_owned())
                    }
                    rusqlite::types::ValueRef::Blob(_) => SqlValue::Null,
                };
                out.insert(name.clone(), value);
            }
            rows_out.push(out);
        }
        Ok(rows_out)
    }
}

/// Decorator proving *which* statements a handler ran — the "no items scan
/// on an unchanged workspace" criterion is a statement-sequence property.
struct RecordingSql<'a> {
    inner: &'a dyn Sql,
    statements: RefCell<Vec<String>>,
}

impl<'a> RecordingSql<'a> {
    fn new(inner: &'a dyn Sql) -> Self {
        RecordingSql {
            inner,
            statements: RefCell::new(Vec::new()),
        }
    }
}

impl Sql for RecordingSql<'_> {
    fn exec(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, SqlError> {
        self.statements.borrow_mut().push(sql.to_string());
        self.inner.exec(sql, params)
    }
}

fn post(sql: &dyn Sql, body: &str, now_ms: i64) -> ApiResponse {
    handle(
        &ApiRequest {
            method: "POST",
            path: "/api/items",
            query: None,
            body: Some(body),
        },
        now_ms,
        sql,
    )
}

fn patch(sql: &dyn Sql, id: &str, body: &str, now_ms: i64) -> ApiResponse {
    let path = format!("/api/items/{id}");
    handle(
        &ApiRequest {
            method: "PATCH",
            path: &path,
            query: None,
            body: Some(body),
        },
        now_ms,
        sql,
    )
}

fn changes(sql: &dyn Sql, query: &str) -> ApiResponse {
    handle(
        &ApiRequest {
            method: "GET",
            path: "/api/changes",
            query: Some(query),
            body: None,
        },
        0,
        sql,
    )
}

fn item(resp: &ApiResponse) -> Item {
    serde_json::from_str(&resp.body).expect("body is an Item")
}

fn meta_version(sql: &dyn Sql) -> i64 {
    sql.exec("SELECT version FROM meta WHERE id = 1", &[]).unwrap()[0]
        .get("version")
        .unwrap()
        .as_i64()
        .unwrap()
}

// ------------------------------------------------------- create (POST)

#[test]
fn create_returns_201_with_stamped_item() {
    let sql = RusqliteSql::new();
    let resp = post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    assert_eq!(resp.status, 201, "{}", resp.body);
    let created = item(&resp);
    assert_eq!(created.id, "a-1");
    assert_eq!(created.seq, Some(1));
    assert_eq!(created.version, 1);
    assert_eq!(created.created_at, 1000);
    assert_eq!(created.updated_at, 1000);
    assert_eq!(created.stage.as_str(), "triage");
    assert_eq!(created.priority, 0);
    assert_eq!(meta_version(&sql), 1);
}

#[test]
fn create_replay_same_id_returns_200_current_item_without_bump() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    let resp = post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 2000);
    assert_eq!(resp.status, 200, "replay is success, not conflict");
    let replayed = item(&resp);
    assert_eq!(replayed.title, "hello");
    assert_eq!(replayed.created_at, 1000, "the original row, untouched");
    assert_eq!(meta_version(&sql), 1, "no version bump on replay");
    let rows = sql.exec("SELECT id FROM items", &[]).unwrap();
    assert_eq!(rows.len(), 1, "no duplicate row");
}

#[test]
fn seq_mints_monotonically() {
    let sql = RusqliteSql::new();
    for (i, id) in ["a", "b", "c"].iter().enumerate() {
        let resp = post(&sql, &format!(r#"{{"id": "{id}", "title": "t"}}"#), 0);
        assert_eq!(item(&resp).seq, Some(i as i64 + 1));
    }
}

#[test]
fn create_accepts_the_full_field_set() {
    let sql = RusqliteSql::new();
    let resp = post(
        &sql,
        r#"{"id": "a-1", "title": "hello", "description": "d", "stage": "ready",
            "size": "quick", "energy": "high", "context": "@computer", "priority": 3,
            "project_id": "p-1", "project_pos": 2, "due_date": "2026-08-15",
            "scheduled_date": "2026-08-10", "source": "google-tasks/v1",
            "source_key": "gt-9", "source_url": "https://example.test/t/9"}"#,
        500,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    let created = item(&resp);
    assert_eq!(created.stage.as_str(), "ready");
    assert_eq!(created.size.map(|s| s.as_str()), Some("quick"));
    assert_eq!(created.energy.map(|e| e.as_str()), Some("high"));
    assert_eq!(created.priority, 3);
    assert_eq!(created.source.as_deref(), Some("google-tasks/v1"));
}

#[test]
fn create_validation_rejects_bad_input() {
    let sql = RusqliteSql::new();
    for (body, why) in [
        (r#"{"id": "", "title": "t"}"#, "empty id"),
        (r#"{"id": "a", "title": ""}"#, "empty title"),
        (r#"{"id": "a", "title": "t", "priority": 5}"#, "priority out of range"),
        (r#"{"id": "a", "title": "t", "stage": "backlog"}"#, "stage outside the six"),
        (r#"not json"#, "malformed JSON"),
    ] {
        let resp = post(&sql, body, 0);
        assert_eq!(resp.status, 400, "{why}: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 0, "no write happened");
}

// -------------------------------------------------------- patch (PATCH)

#[test]
fn patch_fresh_version_applies_and_bumps() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    let resp = patch(
        &sql,
        "a-1",
        r#"{"expected_version": 1, "title": "renamed", "stage": "in_progress"}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let updated = item(&resp);
    assert_eq!(updated.title, "renamed");
    assert_eq!(updated.stage.as_str(), "in_progress");
    assert_eq!(updated.version, 2);
    assert_eq!(updated.updated_at, 2000);
    assert_eq!(updated.created_at, 1000, "created_at never restamps");
    assert_eq!(meta_version(&sql), 2);
}

#[test]
fn patch_stale_version_409_carries_current_entity() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    let resp = patch(&sql, "a-1", r#"{"expected_version": 99, "title": "x"}"#, 2000);
    assert_eq!(resp.status, 409);
    let conflict: ConflictResponse = serde_json::from_str(&resp.body).unwrap();
    assert_eq!(conflict.error, "version_conflict");
    assert_eq!(conflict.current.title, "hello", "the current entity, unmodified");
    assert_eq!(conflict.current.version, 1);
    assert_eq!(meta_version(&sql), 1, "a stale write bumps nothing");
}

#[test]
fn patch_unknown_id_404() {
    let sql = RusqliteSql::new();
    let resp = patch(&sql, "ghost", r#"{"expected_version": 1}"#, 0);
    assert_eq!(resp.status, 404);
}

#[test]
fn patch_explicit_null_clears_and_absent_leaves() {
    let sql = RusqliteSql::new();
    post(
        &sql,
        r#"{"id": "a-1", "title": "hello", "description": "keep?", "context": "@computer"}"#,
        1000,
    );
    let resp = patch(
        &sql,
        "a-1",
        r#"{"expected_version": 1, "description": null}"#,
        2000,
    );
    let updated = item(&resp);
    assert_eq!(updated.description, None, "explicit null clears");
    assert_eq!(
        updated.context.as_deref(),
        Some("@computer"),
        "absent field is untouched"
    );
}

#[test]
fn patch_validation_rejects_bad_input() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    for (body, why) in [
        (r#"{"expected_version": 1, "title": ""}"#, "empty title"),
        (r#"{"expected_version": 1, "priority": 9}"#, "priority out of range"),
        (r#"{"title": "no version"}"#, "missing expected_version"),
        (r#"{"#, "malformed JSON"),
    ] {
        let resp = patch(&sql, "a-1", body, 2000);
        assert_eq!(resp.status, 400, "{why}: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 1, "no write happened");
}

// ------------------------------------------------------- changes (GET)

#[test]
fn changes_since_current_is_empty_and_reads_only_meta() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    let recording = RecordingSql::new(&sql);
    let resp = changes(&recording, "since=1");
    assert_eq!(resp.status, 200);
    let parsed: ChangesResponse = serde_json::from_str(&resp.body).unwrap();
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
    let parsed: ChangesResponse = serde_json::from_str(&resp.body).unwrap();
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
    let parsed: ChangesResponse =
        serde_json::from_str(&changes(&sql, "since=0").body).unwrap();
    assert_eq!(parsed.items.len(), 2);
    assert!(
        parsed.items[0].version < parsed.items[1].version,
        "ordered by version"
    );
}

#[test]
fn changes_since_missing_or_non_numeric_400() {
    let sql = RusqliteSql::new();
    for query in ["", "since=abc", "cursor=1"] {
        let resp = changes(&sql, query);
        assert_eq!(resp.status, 400, "query {query:?}: {}", resp.body);
    }
    let no_query = handle(
        &ApiRequest {
            method: "GET",
            path: "/api/changes",
            query: None,
            body: None,
        },
        0,
        &sql,
    );
    assert_eq!(no_query.status, 400);
}

// ------------------------------------------------------------- routing

#[test]
fn unknown_route_404_and_wrong_method_405() {
    let sql = RusqliteSql::new();
    let unknown = handle(
        &ApiRequest {
            method: "GET",
            path: "/api/nope",
            query: None,
            body: None,
        },
        0,
        &sql,
    );
    assert_eq!(unknown.status, 404);

    for (method, path) in [
        ("GET", "/api/items"),
        ("POST", "/api/items/a-1"),
        ("PATCH", "/api/changes"),
    ] {
        let resp = handle(
            &ApiRequest {
                method,
                path,
                query: None,
                body: Some("{}"),
            },
            0,
            &sql,
        );
        assert_eq!(resp.status, 405, "{method} {path}");
    }
}

// -------------------------------------------------------------- schema

#[test]
fn init_schema_is_idempotent() {
    let sql = RusqliteSql::new(); // ran init once already
    post(&sql, r#"{"id": "a", "title": "t"}"#, 0);
    init_schema(&sql).expect("second init is a no-op");
    assert_eq!(meta_version(&sql), 1, "meta row survives re-init");
    let rows = sql.exec("SELECT id FROM items", &[]).unwrap();
    assert_eq!(rows.len(), 1, "items survive re-init");
}
