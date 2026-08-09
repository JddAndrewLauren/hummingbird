//! `GET /api/changes?since=N` — the delta pull. Grows to all synced tables
//! (and gains `/api/sweep`) later in #114.

use hummingbird_domain::ChangesResponse;

use super::{error, json, query_param, read_meta_version, ApiResponse};
use crate::sql::{Sql, SqlError, SqlValue};

pub fn changes(query: Option<&str>, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let Some(since) = query_param(query, "since").and_then(|v| v.parse::<i64>().ok()) else {
        return Ok(error(400, "validation", "since must be an integer"));
    };

    // The gate: one meta row read answers "anything new?" — the unchanged
    // workspace never touches an entity table (ADR-0008's rows-read
    // argument).
    let version = read_meta_version(sql)?;
    if since >= version {
        return Ok(json(200, &ChangesResponse::empty(version)));
    }

    let rows = sql.exec(
        "SELECT * FROM items WHERE version > ? ORDER BY version",
        &[SqlValue::Integer(since)],
    )?;
    let items = rows
        .iter()
        .map(super::items::item_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json(
        200,
        &ChangesResponse {
            items,
            ..ChangesResponse::empty(version)
        },
    ))
}
