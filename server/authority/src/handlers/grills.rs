//! `POST /api/grills` and `GET /api/grills/:id` (#353, ADR-0023). A Grill is
//! an immutable per-item attachment (decision 2): there is no patch route
//! here, only a create and a by-id read. `resulting_stage` is stored, never
//! recomputed by a reader — it is [`hummingbird_domain::resulting_stage`]'s
//! answer at the moment of the create, the one place this crate is allowed
//! to call that function (#352's "one spelling" precedent).
//!
//! **The transcript never rides the sweep** (decision 4): `changes.rs`
//! reads `grills` with its own `SELECT` naming every column but
//! `transcript`, through [`grill_without_transcript_from_row`] — never the
//! generic `pull` helper's `SELECT *`, and never [`grill_from_row`]. Only
//! this module's own [`get`] handler, answering `GET /api/grills/:id`
//! directly, ever reads the column at all.
//!
//! Both routes are gated by `auth::permitted`'s existing default arm
//! (`_ => matches!(scope, Scope::Device)`) — no new arm; see
//! `authority/tests/handler_fixtures/grills.rs` for the fixture that pins
//! this instead of a dedicated match arm (ADR-0018's precedent, verbatim).

use hummingbird_domain::{CreateGrill, Grill, GrillVerdict, resulting_stage};

use super::{error, json, parse_body, read_meta_version, write_meta_version, ApiResponse};
use crate::codec::{bad_cell, RowReader};
use crate::sql::{Row, Sql, SqlError, SqlValue};

pub fn create(body: Option<&str>, now_ms: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let create: CreateGrill = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if create.id.is_empty() {
        return Ok(error(400, "validation", "id must be non-empty"));
    }

    // Idempotent by client-supplied id, same rule as every other create:
    // a replay is answered with the stored row, no write, no version bump.
    if let Some(row) = select_grill(sql, &create.id)? {
        return Ok(json(200, &grill_from_row(&row)?));
    }

    let Some(item_row) = select_item_stage(sql, &create.item_id)? else {
        return Ok(error(400, "validation", "unknown item_id"));
    };
    let current_stage = item_row;
    let resulting = match resulting_stage(current_stage, create.verdict) {
        Ok(stage) => stage,
        // ADR-0023: Done is out of scope for the whole Grill plan.
        Err(_) => return Ok(error(400, "validation", "item is done")),
    };

    let version = read_meta_version(sql)? + 1;
    let grill = Grill {
        id: create.id,
        item_id: create.item_id,
        transcript: create.transcript,
        summary: create.summary,
        verdict: create.verdict,
        model_proposal: create.model_proposal,
        applied_patch: create.applied_patch,
        resulting_stage: resulting,
        completed_at: now_ms,
        version,
    };
    sql.exec(
        "INSERT INTO grills (id, item_id, transcript, summary, verdict, model_proposal, \
         applied_patch, resulting_stage, completed_at, version) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        &grill_params(&grill),
    )?;
    write_meta_version(sql, version)?;
    Ok(json(201, &grill))
}

/// `GET /api/grills/:id` — the one place a transcript is ever served
/// (ADR-0023 decision 4).
pub fn get(id: &str, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let Some(row) = select_grill(sql, id)? else {
        return Ok(error(404, "not_found", "no such grill"));
    };
    Ok(json(200, &grill_from_row(&row)?))
}

fn select_grill(sql: &dyn Sql, id: &str) -> Result<Option<Row>, SqlError> {
    Ok(sql
        .exec(
            "SELECT * FROM grills WHERE id = ?",
            &[SqlValue::Text(id.to_string())],
        )?
        .into_iter()
        .next())
}

/// The target item's current stage, or `None` if no such item exists.
fn select_item_stage(
    sql: &dyn Sql,
    item_id: &str,
) -> Result<Option<hummingbird_domain::Stage>, SqlError> {
    let Some(row) = sql
        .exec(
            "SELECT stage FROM items WHERE id = ?",
            &[SqlValue::Text(item_id.to_string())],
        )?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    let r = RowReader(&row);
    let stage_text = r.text("stage")?;
    Ok(Some(
        hummingbird_domain::Stage::parse(&stage_text).ok_or_else(|| bad_cell("stage"))?,
    ))
}

/// The INSERT's parameter list, in exactly its column order.
fn grill_params(grill: &Grill) -> Vec<SqlValue> {
    vec![
        SqlValue::Text(grill.id.clone()),
        SqlValue::Text(grill.item_id.clone()),
        SqlValue::Text(grill.transcript.clone()),
        SqlValue::Text(grill.summary.clone()),
        SqlValue::Text(grill.verdict.as_str().to_string()),
        SqlValue::Text(grill.model_proposal.clone()),
        SqlValue::Text(grill.applied_patch.clone()),
        SqlValue::Text(grill.resulting_stage.as_str().to_string()),
        SqlValue::Integer(grill.completed_at),
        SqlValue::Integer(grill.version),
    ]
}

/// The full row, transcript included — [`get`]'s own read only. Never
/// called by `changes.rs`'s `pull`; see [`grill_without_transcript_from_row`]
/// for the shape the sweep actually carries.
pub(crate) fn grill_from_row(row: &Row) -> Result<Grill, SqlError> {
    let r = RowReader(row);
    let verdict_text = r.text("verdict")?;
    let stage_text = r.text("resulting_stage")?;
    Ok(Grill {
        id: r.text("id")?,
        item_id: r.text("item_id")?,
        transcript: r.text("transcript")?,
        summary: r.text("summary")?,
        verdict: GrillVerdict::parse(&verdict_text).ok_or_else(|| bad_cell("verdict"))?,
        model_proposal: r.text("model_proposal")?,
        applied_patch: r.text("applied_patch")?,
        resulting_stage: hummingbird_domain::Stage::parse(&stage_text)
            .ok_or_else(|| bad_cell("resulting_stage"))?,
        completed_at: r.int("completed_at")?,
        version: r.int("version")?,
    })
}

/// Every column [`grill_from_row`] reads **except `transcript`** — paired
/// with `changes.rs`'s dedicated `SELECT` for this table (never the
/// generic `pull`'s `SELECT *`), so the sweep and delta paths never even
/// read the column out of SQLite, let alone serialize it (#353's whole
/// point — defense in depth past "the struct has no field for it").
pub(crate) fn grill_without_transcript_from_row(
    row: &Row,
) -> Result<hummingbird_domain::GrillWithoutTranscript, SqlError> {
    let r = RowReader(row);
    let verdict_text = r.text("verdict")?;
    let stage_text = r.text("resulting_stage")?;
    Ok(hummingbird_domain::GrillWithoutTranscript {
        id: r.text("id")?,
        item_id: r.text("item_id")?,
        summary: r.text("summary")?,
        verdict: GrillVerdict::parse(&verdict_text).ok_or_else(|| bad_cell("verdict"))?,
        model_proposal: r.text("model_proposal")?,
        applied_patch: r.text("applied_patch")?,
        resulting_stage: hummingbird_domain::Stage::parse(&stage_text)
            .ok_or_else(|| bad_cell("resulting_stage"))?,
        completed_at: r.int("completed_at")?,
        version: r.int("version")?,
    })
}
