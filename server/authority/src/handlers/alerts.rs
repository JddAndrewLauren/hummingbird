//! Alerts: the pushed-context lane. `POST /api/alerts` (ingest scope) is a
//! version-blind upsert on `(source, source_key)` — webhook sources cannot
//! track versions, and the source is authoritative for its own fields
//! (ADR-0009 rule 3). `PATCH /api/alerts/:id` (device scope) writes the one
//! human-owned field, `dismissed_at`, under normal CAS.

use hummingbird_domain::{Alert, AlertIngest, AlertPatch};

use super::{
    auth, conflict, empty_status, error, json, parse_body, read_meta_version, write_meta_version,
    ApiResponse,
};
use crate::codec::{RowReader, Sets};
use crate::sql::{Row, Sql, SqlError, SqlValue};

/// `token_source` is the caller's bound source (#145) — `None` for a
/// legacy/raw-seeded ingest token, always `Some` for one minted through
/// `POST /api/admin/tokens`. A payload naming any other source is a 403
/// with an empty body, matching the rest of the scope-matrix error
/// semantics; it never reaches the upsert below.
pub fn ingest(
    body: Option<&str>,
    now_ms: i64,
    token_source: Option<&str>,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    let ingest: AlertIngest = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if ingest.source.is_empty() || ingest.source_key.is_empty() {
        return Ok(error(400, "validation", "source and source_key must be non-empty"));
    }
    if ingest.title.is_empty() {
        return Ok(error(400, "validation", "title must be non-empty"));
    }
    if token_source != Some(ingest.source.as_str()) {
        return Ok(empty_status(403));
    }

    let Some(row) = select_by_identity(sql, &ingest.source, &ingest.source_key)? else {
        // First raise. The id is minted deterministically from the identity
        // — no entropy, and a crashed-and-replayed first raise lands on the
        // same row via the select above.
        let version = read_meta_version(sql)? + 1;
        let alert = Alert {
            id: deterministic_id(&ingest.source, &ingest.source_key),
            source: ingest.source,
            source_key: ingest.source_key,
            title: ingest.title,
            body: ingest.body,
            url: ingest.url,
            severity: ingest.severity,
            raised_at: ingest.raised_at.unwrap_or(now_ms),
            resolved_at: ingest.resolved_at,
            dismissed_at: None,
            expires_at: ingest.expires_at,
            version,
        };
        sql.exec(
            "INSERT INTO alerts (id, source, source_key, title, body, url, severity, \
             raised_at, resolved_at, dismissed_at, expires_at, version) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?)",
            &[
                SqlValue::Text(alert.id.clone()),
                SqlValue::Text(alert.source.clone()),
                SqlValue::Text(alert.source_key.clone()),
                SqlValue::Text(alert.title.clone()),
                SqlValue::from_opt_text(alert.body.as_deref()),
                SqlValue::from_opt_text(alert.url.as_deref()),
                SqlValue::from_opt_text(alert.severity.as_deref()),
                SqlValue::Integer(alert.raised_at),
                SqlValue::from_opt_i64(alert.resolved_at),
                SqlValue::from_opt_i64(alert.expires_at),
                SqlValue::Integer(alert.version),
            ],
        )?;
        write_meta_version(sql, version)?;
        return Ok(json(201, &alert));
    };

    // Re-raise: the source-owned fields are set absolutely from the payload
    // (an absent optional clears — the source stopped sending it), with one
    // deliberate exception: an absent `raised_at` keeps the stored stamp,
    // so an identical replayed payload is a byte-identical state and the
    // upsert a no-op (AC1). `dismissed_at` is human-owned: never touched.
    let current = alert_from_row(&row)?;
    let next = Alert {
        title: ingest.title,
        body: ingest.body,
        url: ingest.url,
        severity: ingest.severity,
        raised_at: ingest.raised_at.unwrap_or(current.raised_at),
        resolved_at: ingest.resolved_at,
        expires_at: ingest.expires_at,
        ..current.clone()
    };
    if next == current {
        return Ok(json(200, &current));
    }

    let version = read_meta_version(sql)? + 1;
    sql.exec(
        "UPDATE alerts SET title = ?, body = ?, url = ?, severity = ?, raised_at = ?, \
         resolved_at = ?, expires_at = ?, version = ? WHERE id = ?",
        &[
            SqlValue::Text(next.title.clone()),
            SqlValue::from_opt_text(next.body.as_deref()),
            SqlValue::from_opt_text(next.url.as_deref()),
            SqlValue::from_opt_text(next.severity.as_deref()),
            SqlValue::Integer(next.raised_at),
            SqlValue::from_opt_i64(next.resolved_at),
            SqlValue::from_opt_i64(next.expires_at),
            SqlValue::Integer(version),
            SqlValue::Text(next.id.clone()),
        ],
    )?;
    write_meta_version(sql, version)?;
    Ok(json(200, &Alert { version, ..next }))
}

/// The device lane: dismiss (or un-dismiss) under CAS — the only alert
/// field a device may write.
pub fn dismiss(
    id: &str,
    body: Option<&str>,
    _now_ms: i64,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    let patch: AlertPatch = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };

    let Some(row) = select_by_id(sql, id)? else {
        return Ok(error(404, "not_found", "no such alert"));
    };
    let current = alert_from_row(&row)?;
    if current.version != patch.expected_version {
        return Ok(conflict(&current));
    }

    let mut sets = Sets::new();
    if let Some(dismissed_at) = patch.dismissed_at {
        sets.set("dismissed_at", SqlValue::from_opt_i64(dismissed_at));
    }
    if sets.is_empty() {
        return Ok(json(200, &current));
    }

    let version = read_meta_version(sql)? + 1;
    sets.set("version", SqlValue::Integer(version));
    let update = sets.update_sql("alerts", "id = ?");
    let mut params = sets.into_params();
    params.push(SqlValue::Text(id.to_string()));
    sql.exec(&update, &params)?;
    write_meta_version(sql, version)?;

    let row = select_by_id(sql, id)?.ok_or_else(|| SqlError {
        message: "row vanished mid-update".into(),
    })?;
    Ok(json(200, &alert_from_row(&row)?))
}

/// `hex(sha256("alert:" + len(source) + ":" + source + ":" + source_key))[..32]`
/// — stable across raises by construction. The length prefix keeps the
/// preimage unambiguous: without it, `("app", "db:prod")` and
/// `("app:db", "prod")` would collide on id while differing on the real
/// identity columns, and the second raise would hit the primary key forever.
fn deterministic_id(source: &str, source_key: &str) -> String {
    let mut id = auth::sha256_hex(&format!("alert:{}:{source}:{source_key}", source.len()));
    id.truncate(32);
    id
}

fn select_by_identity(sql: &dyn Sql, source: &str, source_key: &str) -> Result<Option<Row>, SqlError> {
    Ok(sql
        .exec(
            "SELECT * FROM alerts WHERE source = ? AND source_key = ?",
            &[
                SqlValue::Text(source.to_string()),
                SqlValue::Text(source_key.to_string()),
            ],
        )?
        .into_iter()
        .next())
}

fn select_by_id(sql: &dyn Sql, id: &str) -> Result<Option<Row>, SqlError> {
    Ok(sql
        .exec(
            "SELECT * FROM alerts WHERE id = ?",
            &[SqlValue::Text(id.to_string())],
        )?
        .into_iter()
        .next())
}

pub(super) fn alert_from_row(row: &Row) -> Result<Alert, SqlError> {
    let r = RowReader(row);
    Ok(Alert {
        id: r.text("id")?,
        source: r.text("source")?,
        source_key: r.text("source_key")?,
        title: r.text("title")?,
        body: r.opt_text("body"),
        url: r.opt_text("url"),
        severity: r.opt_text("severity"),
        raised_at: r.int("raised_at")?,
        resolved_at: r.opt_int("resolved_at"),
        dismissed_at: r.opt_int("dismissed_at"),
        expires_at: r.opt_int("expires_at"),
        version: r.int("version")?,
    })
}
