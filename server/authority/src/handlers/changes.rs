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
//! A fifth table appears only *partially*: `alerts` is bounded by
//! ADR-0016's wire horizon ([`ALERT_HORIZON_MS`]). A live alert rides every
//! sweep forever, at any age; a settled one leaves the wire once every
//! stamp that settled it is older than the horizon. The row itself is never
//! deleted — this is a limit on what is transmitted, never on what is kept.
//!
//! A sixth table's columns appear only *partially* too, but along a
//! different axis: `grills` rides every sweep in full **except**
//! `transcript` (ADR-0023 decision 4, #353) — every row, at any age, minus
//! one column, never the reverse of `alerts`' every-column-minus-some-rows
//! narrowing. [`pull_grills_without_transcript`] is its own query rather
//! than the generic [`pull`] for exactly that reason.
//!
//! [`ChangesResponse`]: hummingbird_domain::ChangesResponse

use hummingbird_domain::{settled_at, ChangesResponse};

use super::{error, json, query_param, read_meta_version, ApiResponse};
use crate::sql::{Row, Sql, SqlError, SqlValue};

/// ADR-0016: how far back the wire carries a *settled* alert. A named
/// `const` on `ALARM_INTERVAL_MS`'s precedent — readable rather than a
/// buried literal — and deliberately not a `settings` row: `settings` is
/// device-writable and has no DELETE, so an accidental `0` would be an
/// unrecoverable change to what every device can see.
pub const ALERT_HORIZON_MS: i64 = 90 * 24 * 60 * 60 * 1_000;

pub fn changes(query: Option<&str>, now_ms: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let Some(since) = query_param(query, "since").and_then(|v| v.parse::<i64>().ok()) else {
        return Ok(error(400, "validation", "since must be an integer"));
    };
    changes_since(since, now_ms, sql)
}

pub fn sweep(now_ms: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    changes_since(0, now_ms, sql)
}

fn changes_since(since: i64, now_ms: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
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
        // The horizon is applied here, inside the one code path the sweep
        // and the delta share, so their byte-for-byte agreement holds by
        // construction. On the delta it is inert *in practice*: every
        // writer stamps the settling field from its own clock at the
        // moment of the write, so a row above the cursor carries a recent
        // stamp and passes. That is a fact about the writers, not a
        // guarantee — the cursor measures write order, never wall-clock
        // age — so a settlement stamped with a historical time is omitted
        // from the delta that would have carried it, and the device's next
        // sweep is what retires it. ADR-0016, "The second gap".
        alerts: pull(sql, since, "alerts", "id", super::alerts::alert_from_row)?
            .into_iter()
            .filter(|alert| {
                settled_at(
                    alert.raised_at,
                    alert.resolved_at,
                    alert.dismissed_at,
                    alert.expires_at,
                    now_ms,
                )
                .is_none_or(|settled| settled >= now_ms - ALERT_HORIZON_MS)
            })
            .collect(),
        context_snapshots: pull(
            sql,
            since,
            "context_snapshots",
            "source, key",
            super::snapshots::snapshot_from_row,
        )?,
        settings: pull(sql, since, "settings", "key", super::settings::setting_from_row)?,
        rules: pull(sql, since, "rules", "id", super::rules::rule_from_row)?,
        grills: pull_grills_without_transcript(sql, since)?,
    };
    Ok(json(200, &response))
}

/// `grills`' own pull, deliberately not routed through the generic
/// [`pull`] helper: `pull` issues `SELECT * FROM {table}`, which would read
/// `transcript` out of SQLite on every sweep even though nothing downstream
/// serializes it. Naming every column but `transcript` here means the
/// sweep and delta paths never load the column at all — the anti-goal
/// (#353: "the easiest thing to regress") held by the query text itself,
/// not only by [`GrillWithoutTranscript`] lacking the field.
fn pull_grills_without_transcript(
    sql: &dyn Sql,
    since: i64,
) -> Result<Vec<hummingbird_domain::GrillWithoutTranscript>, SqlError> {
    sql.exec(
        "SELECT id, item_id, summary, verdict, model_proposal, applied_patch, \
         resulting_stage, completed_at, version FROM grills \
         WHERE version > ? ORDER BY version, id",
        &[SqlValue::Integer(since)],
    )?
    .iter()
    .map(super::grill_without_transcript_from_row)
    .collect()
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

