//! `GET /api/changes?since=N` (the delta pull) and `GET /api/sweep` (the
//! correctness backstop). The sweep *is* the delta called with `since = 0`
//! — one code path, so their byte-for-byte agreement (#114 acceptance
//! criterion 4) holds by construction; a fixture pins it anyway.
//!
//! Four tables never appear. `tokens` is per-writer machinery that never
//! syncs to clients, and the `meta` counter is the response's `version`
//! field. `push_targets` and `deliveries` are out for the structural
//! reason: neither carries a `version` column (#131), so neither can sit
//! on a cursor at all — they are server-side machinery, not delta-pulled
//! records. See [`ChangesResponse`]'s twin note.
//!
//! [`ChangesResponse`]: hummingbird_domain::ChangesResponse

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
        projects: pull(sql, since, "projects", "id", super::projects::project_from_row)?,
        routes: pull(sql, since, "routes", "project_id", super::routes::route_from_row)?,
        fog: pull(sql, since, "fog", "id", super::fog::fog_from_row)?,
        items: pull(sql, since, "items", "id", super::items::item_from_row)?,
        steps: pull(sql, since, "steps", "id", super::steps::step_from_row)?,
        blocked_by: pull(sql, since, "blocked_by", "item_id, blocker_id", super::blocked_by::edge_from_row)?,
        alerts: pull(sql, since, "alerts", "id", super::alerts::alert_from_row)?,
        context_snapshots: pull(sql, since, "context_snapshots", "source, key", snapshot_from_row)?,
        settings: pull(sql, since, "settings", "key", super::settings::setting_from_row)?,
        rules: pull(sql, since, "rules", "id", super::rules::rule_from_row)?,
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
    .map(from_row)
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
