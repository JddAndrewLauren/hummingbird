//! The shared rig: real SQLite (rusqlite in memory) behind the same [`Sql`]
//! seam the Durable Object drives, a statement-recording decorator, and the
//! request helpers every suite builds on. Zero live credentials.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU8, Ordering};

use hummingbird_authority::{
    handle, init_schema, ApiRequest, ApiResponse, Entropy, HandleContext, Row, SqlError, SqlValue,
};
use hummingbird_domain::{Alert, Item, Rule};
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
            // Seeded with no `source`: a deliberate stand-in for the
            // pre-#145 unbound ingest token. `ingest_alert` rebinds
            // `rig-ingest` to whatever source each fixture's payload names,
            // so existing suites (each exercising a different source
            // string) keep passing under the new binding check without
            // having to agree on one shared source.
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

/// Coerces every `Integer` result cell to a whole `Real` — standing in for
/// a Durable Object cursor, which (per `SqlValue::as_i64`'s own doc) may
/// surface an INTEGER column that way. rusqlite never does this on its own
/// (#166), so without this decorator the CI backend can't reproduce the
/// hazard a naive `SqlValue == SqlValue` comparison would hide: `Integer(2)
/// != Real(2.0)`, so a value-identical patch would wrongly be seen as
/// "changed" and bump. Every fixture using this decorator only wraps the
/// PATCH call under test — seeding and post-call assertions still go
/// through the plain rusqlite handle, so only the read the handler's
/// current-value comparison depends on is affected.
pub struct RealCoercingSql<'a> {
    pub inner: &'a dyn Sql,
}

impl<'a> RealCoercingSql<'a> {
    pub fn new(inner: &'a dyn Sql) -> Self {
        RealCoercingSql { inner }
    }
}

impl Sql for RealCoercingSql<'_> {
    fn exec(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, SqlError> {
        let rows = self.inner.exec(sql, params)?;
        Ok(rows
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|(col, value)| {
                        let value = match value {
                            SqlValue::Integer(n) => SqlValue::Real(n as f64),
                            other => other,
                        };
                        (col, value)
                    })
                    .collect()
            })
            .collect())
    }
}

// ------------------------------------------------------ request helpers

/// The lowest-level request: explicit authorization header and admin
/// secret. Every other helper reduces to this.
// Eight arguments by design: this helper's whole job is to leave every axis
// of an `ApiRequest` explicit at one call site, so a fixture can vary any
// one of them. The narrower helpers below are the ergonomic front doors.
#[allow(clippy::too_many_arguments)]
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

pub fn post_rule(sql: &dyn Sql, body: &str, now_ms: i64) -> ApiResponse {
    post_to(sql, "/api/rules", body, now_ms)
}

pub fn patch_rule(sql: &dyn Sql, id: &str, body: &str, now_ms: i64) -> ApiResponse {
    patch_at(sql, &format!("/api/rules/{id}"), body, now_ms)
}

/// Webhook ingest — the one route that speaks with the ingest scope. Rebinds
/// the rig's ingest token (#145) to whatever `source` the body names before
/// every call, so callers can keep using whichever source string their
/// fixture cares about without a 403 from the new binding check; use
/// `req_as` with `INGEST_TOKEN` directly (as the mismatch tests do) to
/// exercise a stale binding instead.
pub fn ingest_alert(sql: &dyn Sql, body: &str, now_ms: i64) -> ApiResponse {
    let source = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("source").and_then(|s| s.as_str()).map(str::to_string))
        .unwrap_or_default();
    bind_ingest_token(sql, &source);
    req_as(sql, INGEST_TOKEN, "POST", "/api/alerts", None, Some(body), now_ms)
}

/// Rebind `rig-ingest`'s bound source directly through the seam (mint's own
/// pairing validation is exercised in `admin_tokens.rs`, not here).
pub fn bind_ingest_token(sql: &dyn Sql, source: &str) {
    sql.exec(
        "UPDATE tokens SET source = ? WHERE id = 'rig-ingest'",
        &[SqlValue::Text(source.to_string())],
    )
    .expect("rebinding the rig ingest token");
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

pub fn rule(resp: &ApiResponse) -> Rule {
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

/// Create a rule through the handler and return it.
pub fn seed_rule(sql: &dyn Sql, id: &str) -> Rule {
    let resp = post_rule(
        sql,
        &format!(
            r#"{{"id": "{id}", "name": "seeded {id}", "conditions": [], "severity": "normal", "tier": "normal"}}"#
        ),
        0,
    );
    assert!(
        resp.status == 201 || resp.status == 200,
        "rule seed failed: {}",
        resp.body
    );
    rule(&resp)
}

/// `push_targets` has no #131 write handler (that HTTP surface is #139), so
/// it is seeded through the seam directly, like `alerts` was in #114.
pub fn seed_push_target_raw(sql: &dyn Sql, id: &str, name: &str) {
    sql.exec(
        "INSERT INTO push_targets (id, name, platform, fcm_token, created_at) \
         VALUES (?, ?, 'android', 'tok', 0)",
        &[SqlValue::Text(id.into()), SqlValue::Text(name.into())],
    )
    .unwrap();
}

/// `deliveries` has no #131 write handler either (#139); seeded raw for the
/// dedupe-key fixtures.
pub fn seed_delivery_raw(
    sql: &dyn Sql,
    id: &str,
    alert_id: &str,
    rule_id: &str,
    generation: i64,
    severity: &str,
) -> Result<(), SqlError> {
    sql.exec(
        "INSERT INTO deliveries (id, alert_id, rule_id, generation, severity, tier, sent_at) \
         VALUES (?, ?, ?, ?, ?, 'normal', 0)",
        &[
            SqlValue::Text(id.into()),
            SqlValue::Text(alert_id.into()),
            SqlValue::Text(rule_id.into()),
            SqlValue::Integer(generation),
            SqlValue::Text(severity.into()),
        ],
    )
    .map(|_| ())
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

/// Seeds an alert with full control over its lifecycle columns, for
/// `delivery.rs`'s tests, which exercise ADR-0014's live predicate and the
/// severity ratchet directly. Only the row's identity (`id`) needs to
/// exist for `deliveries`' foreign key — callers that want to exercise an
/// escalation or a later generation build a second [`Alert`] value with the
/// same `id` in memory (matching how #138 will hand `deliver` its own
/// freshly ratcheted view) rather than mutating this row.
#[allow(clippy::too_many_arguments)]
pub fn seed_alert_full_raw(
    sql: &dyn Sql,
    id: &str,
    severity: Option<&str>,
    raised_at: i64,
    resolved_at: Option<i64>,
    dismissed_at: Option<i64>,
    expires_at: Option<i64>,
) -> Alert {
    let version = meta_version(sql) + 1;
    sql.exec(
        "INSERT INTO alerts (id, source, source_key, title, severity, raised_at, resolved_at, \
         dismissed_at, expires_at, version) VALUES (?, 'test-source/v1', ?, 'seeded alert', \
         ?, ?, ?, ?, ?, ?)",
        &[
            SqlValue::Text(id.into()),
            SqlValue::Text(id.into()),
            SqlValue::from_opt_text(severity),
            SqlValue::Integer(raised_at),
            SqlValue::from_opt_i64(resolved_at),
            SqlValue::from_opt_i64(dismissed_at),
            SqlValue::from_opt_i64(expires_at),
            SqlValue::Integer(version),
        ],
    )
    .unwrap();
    sql.exec(
        "UPDATE meta SET version = ? WHERE id = 1",
        &[SqlValue::Integer(version)],
    )
    .unwrap();
    Alert {
        id: id.into(),
        source: "test-source/v1".into(),
        source_key: id.into(),
        title: "seeded alert".into(),
        body: None,
        url: None,
        severity: severity.map(str::to_string),
        raised_at,
        resolved_at,
        dismissed_at,
        expires_at,
        version,
    }
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
