//! The shared rig: real SQLite (rusqlite in memory) behind the same [`Sql`]
//! seam the Durable Object drives, a statement-recording decorator, and the
//! request helpers every suite builds on. Zero live credentials.

use std::cell::RefCell;

use hummingbird_authority::{handle, init_schema, ApiRequest, ApiResponse, Row, SqlError, SqlValue};
use hummingbird_domain::Item;

// Re-exported so every suite gets the trait (for `sql.exec`) with `rig::*`.
pub use hummingbird_authority::Sql;

pub struct RusqliteSql {
    pub conn: rusqlite::Connection,
}

impl RusqliteSql {
    pub fn new() -> Self {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory sqlite opens");
        // Deliberately ON (rusqlite's bundled SQLite defaults it on; plain
        // SQLite defaults it off and the DO's posture is unverified): the
        // handlers validate every referent explicitly and answer 400, so
        // here the constraint is a backstop — a missed validation fails a
        // test as a 500 instead of passing silently.
        conn.pragma_update(None, "foreign_keys", true)
            .expect("pragma applies");
        let sql = RusqliteSql { conn };
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
pub struct RecordingSql<'a> {
    pub inner: &'a dyn Sql,
    pub statements: RefCell<Vec<String>>,
}

impl<'a> RecordingSql<'a> {
    pub fn new(inner: &'a dyn Sql) -> Self {
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

// ------------------------------------------------------ request helpers

/// The generic request: every suite's per-entity wrappers reduce to this.
pub fn req(
    sql: &dyn Sql,
    method: &str,
    path: &str,
    query: Option<&str>,
    body: Option<&str>,
    now_ms: i64,
) -> ApiResponse {
    handle(
        &ApiRequest {
            method,
            path,
            query,
            body,
        },
        now_ms,
        sql,
    )
}

/// `POST` to a collection path with a JSON body.
pub fn post_to(sql: &dyn Sql, path: &str, body: &str, now_ms: i64) -> ApiResponse {
    req(sql, "POST", path, None, Some(body), now_ms)
}

/// `PATCH` an entity path with a JSON body.
pub fn patch_at(sql: &dyn Sql, path: &str, body: &str, now_ms: i64) -> ApiResponse {
    req(sql, "PATCH", path, None, Some(body), now_ms)
}

// S0-era items shorthands, used across suites.

pub fn post(sql: &dyn Sql, body: &str, now_ms: i64) -> ApiResponse {
    post_to(sql, "/api/items", body, now_ms)
}

pub fn patch(sql: &dyn Sql, id: &str, body: &str, now_ms: i64) -> ApiResponse {
    patch_at(sql, &format!("/api/items/{id}"), body, now_ms)
}

pub fn changes(sql: &dyn Sql, query: &str) -> ApiResponse {
    req(sql, "GET", "/api/changes", Some(query), None, 0)
}

pub fn sweep(sql: &dyn Sql) -> ApiResponse {
    req(sql, "GET", "/api/sweep", None, None, 0)
}

pub fn put_setting(sql: &dyn Sql, key: &str, body: &str, now_ms: i64) -> ApiResponse {
    req(sql, "PUT", &format!("/api/settings/{key}"), None, Some(body), now_ms)
}

// -------------------------------------------------------- body helpers

/// Deserialize a response body into any DTO.
pub fn body_as<T: serde::de::DeserializeOwned>(resp: &ApiResponse) -> T {
    serde_json::from_str(&resp.body)
        .unwrap_or_else(|e| panic!("body parses: {e}: {}", resp.body))
}

pub fn item(resp: &ApiResponse) -> Item {
    body_as(resp)
}

pub fn meta_version(sql: &dyn Sql) -> i64 {
    sql.exec("SELECT version FROM meta WHERE id = 1", &[]).unwrap()[0]
        .get("version")
        .unwrap()
        .as_i64()
        .unwrap()
}

// -------------------------------------------------------- seed helpers

/// Create a project through the handler and return its id.
pub fn seed_project(sql: &dyn Sql, id: &str) -> i64 {
    let resp = post_to(
        sql,
        "/api/projects",
        &format!(r#"{{"id": "{id}", "name": "seeded {id}"}}"#),
        0,
    );
    assert!(
        resp.status == 201 || resp.status == 200,
        "project seed failed: {}",
        resp.body
    );
    body_as::<hummingbird_domain::Project>(&resp).version
}

/// Create an item through the handler and return its version.
pub fn seed_item(sql: &dyn Sql, id: &str) -> i64 {
    let resp = post(sql, &format!(r#"{{"id": "{id}", "title": "seeded {id}"}}"#), 0);
    assert!(
        resp.status == 201 || resp.status == 200,
        "item seed failed: {}",
        resp.body
    );
    item(&resp).version
}

/// The two tables without a #114 write handler are seeded through the seam
/// directly, stamping the next workspace version the way a handler would.

pub fn seed_alert_raw(sql: &dyn Sql, id: &str, source: &str, source_key: &str) -> i64 {
    let version = meta_version(sql) + 1;
    sql.exec(
        "INSERT INTO alerts (id, source, source_key, title, raised_at, version) \
         VALUES (?, ?, ?, 'seeded alert', 100, ?)",
        &[
            SqlValue::Text(id.into()),
            SqlValue::Text(source.into()),
            SqlValue::Text(source_key.into()),
            SqlValue::Integer(version),
        ],
    )
    .unwrap();
    sql.exec(
        "UPDATE meta SET version = ? WHERE id = 1",
        &[SqlValue::Integer(version)],
    )
    .unwrap();
    version
}

pub fn seed_snapshot_raw(sql: &dyn Sql, source: &str, key: &str) -> i64 {
    let version = meta_version(sql) + 1;
    sql.exec(
        "INSERT INTO context_snapshots (source, key, payload, fetched_at, version) \
         VALUES (?, ?, '{\"gauge\": 1}', 100, ?)",
        &[
            SqlValue::Text(source.into()),
            SqlValue::Text(key.into()),
            SqlValue::Integer(version),
        ],
    )
    .unwrap();
    sql.exec(
        "UPDATE meta SET version = ? WHERE id = 1",
        &[SqlValue::Integer(version)],
    )
    .unwrap();
    version
}
