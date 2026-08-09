//! The shared rig: real SQLite (rusqlite in memory) behind the same [`Sql`]
//! seam the Durable Object drives, a statement-recording decorator, and the
//! request helpers every suite builds on. Zero live credentials.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU8, Ordering};

use hummingbird_authority::{
    handle, init_schema, ApiRequest, ApiResponse, Entropy, HandleContext, Row, SqlError, SqlValue,
};
use hummingbird_domain::Item;
use sha2::{Digest, Sha256};

// Re-exported so every suite gets the trait (for `sql.exec`) with `rig::*`.
pub use hummingbird_authority::Sql;

/// The rig's admin secret, injected into every request's context.
pub const ADMIN_SECRET: &str = "test-admin-secret";

/// Pre-seeded per-scope tokens: rows inserted with precomputed hashes so
/// every suite can speak with any scope without minting first. The mint
/// path itself is exercised in `admin_tokens.rs`.
pub const DEVICE_TOKEN: &str = "hb_rig_device_token";
pub const SWEEPER_TOKEN: &str = "hb_rig_sweeper_token";
pub const INGEST_TOKEN: &str = "hb_rig_ingest_token";

pub fn sha256_hex(input: &str) -> String {
    Sha256::digest(input.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Deterministic but distinct per call: each fill draws a fresh base from a
/// process-wide counter, so two mints in one test never collide on a hash.
pub struct TestEntropy;

static ENTROPY_BASE: AtomicU8 = AtomicU8::new(1);

impl Entropy for TestEntropy {
    fn fill(&self, buf: &mut [u8]) {
        let base = ENTROPY_BASE.fetch_add(1, Ordering::Relaxed);
        for (i, byte) in buf.iter_mut().enumerate() {
            *byte = base.wrapping_add(i as u8);
        }
    }
}

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
        for (id, scope, plaintext) in [
            ("rig-device", "device", DEVICE_TOKEN),
            ("rig-sweeper", "sweeper", SWEEPER_TOKEN),
            ("rig-ingest", "ingest", INGEST_TOKEN),
        ] {
            sql.exec(
                "INSERT INTO tokens (id, name, scope, token_hash, created_at) \
                 VALUES (?, ?, ?, ?, 0)",
                &[
                    SqlValue::Text(id.into()),
                    SqlValue::Text(format!("rig {scope}")),
                    SqlValue::Text(scope.into()),
                    SqlValue::Text(sha256_hex(plaintext)),
                ],
            )
            .expect("token seed inserts");
        }
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

/// The lowest-level request: explicit authorization header and admin
/// secret. Every other helper reduces to this.
pub fn req_with(
    sql: &dyn Sql,
    authorization: Option<&str>,
    admin_secret: Option<&str>,
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
            authorization,
        },
        &HandleContext {
            now_ms,
            admin_secret,
            entropy: &TestEntropy,
        },
        sql,
    )
}

/// A request carrying the given bearer token.
pub fn req_as(
    sql: &dyn Sql,
    token: &str,
    method: &str,
    path: &str,
    query: Option<&str>,
    body: Option<&str>,
    now_ms: i64,
) -> ApiResponse {
    let header = format!("Bearer {token}");
    req_with(sql, Some(&header), Some(ADMIN_SECRET), method, path, query, body, now_ms)
}

/// The default request: the rig's device token — most of the API is
/// device-scoped.
pub fn req(
    sql: &dyn Sql,
    method: &str,
    path: &str,
    query: Option<&str>,
    body: Option<&str>,
    now_ms: i64,
) -> ApiResponse {
    req_as(sql, DEVICE_TOKEN, method, path, query, body, now_ms)
}

/// An admin-lane request: the bearer is the admin secret itself.
pub fn req_admin(
    sql: &dyn Sql,
    method: &str,
    path: &str,
    body: Option<&str>,
    now_ms: i64,
) -> ApiResponse {
    let header = format!("Bearer {ADMIN_SECRET}");
    req_with(sql, Some(&header), Some(ADMIN_SECRET), method, path, None, body, now_ms)
}

/// A deliberately unauthenticated request.
pub fn req_anon(
    sql: &dyn Sql,
    method: &str,
    path: &str,
    query: Option<&str>,
    body: Option<&str>,
) -> ApiResponse {
    req_with(sql, None, Some(ADMIN_SECRET), method, path, query, body, 0)
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

/// Webhook ingest — the one route that speaks with the ingest scope.
pub fn ingest_alert(sql: &dyn Sql, body: &str, now_ms: i64) -> ApiResponse {
    req_as(sql, INGEST_TOKEN, "POST", "/api/alerts", None, Some(body), now_ms)
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
