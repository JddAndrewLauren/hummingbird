//! `GET /api/changes?since=N` (the delta pull) and `GET /api/sweep` (the
//! correctness backstop). The sweep *is* the delta called with `since = 0`
//! — one code path, so their byte-for-byte agreement (#114 acceptance
//! criterion 4) holds by construction; a fixture pins it anyway.
//!
//! `tokens` and `meta` never appear: tokens are per-writer machinery, and
//! the meta counter is the response's `version` field.

use hummingbird_domain::{ChangesResponse, ContextSnapshot};

use super::{error, json, query_param, read_meta_version, ApiResponse};
use crate::codec::RowReader;
use crate::sql::{Row, Sql, SqlError, SqlValue};

pub fn changes(query: Option<&str>, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let Some(since) = query_param(query, "since").and_then(|v| v.parse::<i64>().ok()) else {
        return Ok(error(400, "validation", "since must be an integer"));
    };
    changes_since(since, sql)
}

pub fn sweep(sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    changes_since(0, sql)
}

fn changes_since(since: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    // The gate: one meta row read answers "anything new?" — the unchanged
    // workspace never touches an entity table (ADR-0008's rows-read
    // argument).
    let version = read_meta_version(sql)?;
    if since >= version {
        return Ok(json(200, &ChangesResponse::empty(version)));
    }

    // Every query carries a deterministic total order: rows can share a
    // version stamp (a project and its Route), so the secondary key is the
    // table's primary key.
    let response = ChangesResponse {
        version,
        projects: pull(sql, since, "projects", "id", |r| {
            super::projects::project_from_row(r)
        })?,
        routes: pull(sql, since, "routes", "project_id", |r| {
            super::routes::route_from_row(r)
        })?,
        fog: pull(sql, since, "fog", "id", |r| super::fog::fog_from_row(r))?,
        items: pull(sql, since, "items", "id", |r| super::items::item_from_row(r))?,
        steps: pull(sql, since, "steps", "id", |r| super::steps::step_from_row(r))?,
        blocked_by: pull(sql, since, "blocked_by", "item_id, blocker_id", |r| {
            super::blocked_by::edge_from_row(r)
        })?,
        alerts: pull(sql, since, "alerts", "id", |r| super::alerts::alert_from_row(r))?,
        context_snapshots: pull(sql, since, "context_snapshots", "source, key", |r| {
            snapshot_from_row(r)
        })?,
        settings: pull(sql, since, "settings", "key", |r| {
            super::settings::setting_from_row(r)
        })?,
    };
    Ok(json(200, &response))
}

fn pull<T>(
    sql: &dyn Sql,
    since: i64,
    table: &str,
    pk: &str,
    from_row: impl Fn(&Row) -> Result<T, SqlError>,
) -> Result<Vec<T>, SqlError> {
    sql.exec(
        &format!("SELECT * FROM {table} WHERE version > ? ORDER BY version, {pk}"),
        &[SqlValue::Integer(since)],
    )?
    .iter()
    .map(|row| from_row(row))
    .collect()
}

/// `context_snapshots` has no write handler in #114 (the server-polled lane
/// wires post-cutover), so its row mapping lives with its only reader.
fn snapshot_from_row(row: &Row) -> Result<ContextSnapshot, SqlError> {
    let r = RowReader(row);
    Ok(ContextSnapshot {
        source: r.text("source")?,
        key: r.text("key")?,
        payload: r.text("payload")?,
        fetched_at: r.int("fetched_at")?,
        version: r.int("version")?,
    })
}
