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
//!
//! **#354: the atomic completion mutation follows exactly the model
//! `sql.rs`'s own module doc records — no exception for this handler.**
//! [`create`] can genuinely touch three tables (`grills`, `items`, `steps`)
//! in one request, so every fallible check (unknown item, a Done item, a
//! stale `expected_version`) is resolved *before* [`write_completion`] runs
//! a single `exec` — the same "no reachable failure between statements"
//! discipline every other multi-row write in this crate already relies on
//! (a project born with its Route, an item born with a `seq`). That
//! discipline, plus the DO's single-threaded write coalescing, is this
//! crate's whole atomicity story; this handler adds nothing to it and
//! claims nothing beyond it.
//!
//! **An earlier version of this handler wrapped [`write_completion`] in an
//! explicit `BEGIN`/`COMMIT`/`ROLLBACK`, issued as raw SQL through
//! [`Sql::exec`]. That was wrong and has been removed.** Verified against a
//! real SQLite-backed Durable Object (`wrangler dev`, not just the
//! rusqlite-backed fixture rig): `SqlStorage::exec` rejects a `BEGIN`
//! statement outright —
//!
//! ```text
//! Error: To execute a transaction, please use the state.storage.transaction()
//! or state.storage.transactionSync() APIs instead of the SQL BEGIN
//! TRANSACTION or SAVEPOINT statements. The JavaScript API is safer because
//! it will automatically roll back on exceptions, and because it interacts
//! correctly with Durable Objects' automatic atomic write coalescing.
//! ```
//!
//! — which every fixture test, the wasm32 build check, and CI all stayed
//! green against, because none of them exercise the real Durable Object
//! backend (`server/worker` has no test harness — this is exactly the class
//! of bug that fact warns about). `state.storage().transactionSync(...)` is
//! not a substitute available here: `worker` 0.8.5 (the latest published
//! version as of this writing) binds no `transactionSync` at all — neither
//! on `worker::Storage`/`worker_sys::DurableObjectStorage` nor anywhere
//! else — and reaching it would mean either an unsafe, layout-dependent
//! transmute of a private wrapper field (`worker::durable::State`/`Storage`
//! expose no accessor to their inner `js_sys`-derived value) or a hand
//! -rolled JS reflection call with no test harness to catch a future
//! breakage — precisely the fragile, untested-shim risk CLAUDE.md's "the
//! wasm32 worker build stays thin" rule exists to keep out of
//! `server/worker`. So there is no transaction primitive here at all, by
//! design, same as every other handler in this crate: a genuine platform
//! I/O fault between two of [`write_completion`]'s statements is an
//! accepted, undifferentiated risk this handler does not try to cover,
//! identical to the risk a project-plus-Route create or an item-plus-`seq`
//! create already carries.

use hummingbird_domain::{CreateGrill, Grill, GrillVerdict, Item, resulting_stage};

use super::{conflict, error, json, parse_body, read_meta_version, write_meta_version, ApiResponse};
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
    // a replay is answered with the stored row, no write, no version bump
    // — even if the item has since moved on from the `expected_version`
    // this replay's body still names, because nothing about this replay
    // writes anything at all.
    if let Some(row) = select_grill(sql, &create.id)? {
        return Ok(json(200, &grill_from_row(&row)?));
    }

    let Some(item) = select_item(sql, &create.item_id)? else {
        return Ok(error(400, "validation", "unknown item_id"));
    };
    let resulting = match resulting_stage(item.stage, create.verdict) {
        Ok(stage) => stage,
        // ADR-0023: Done is out of scope for the whole Grill plan.
        Err(_) => return Ok(error(400, "validation", "item is done")),
    };
    if item.version != create.expected_version {
        return Ok(conflict(&item));
    }

    // Every fallible check is resolved above this line — nothing past here
    // can fail for a reason a retry would fix, only a genuine backend
    // fault, which is exactly what the transaction below guards against.
    let version = read_meta_version(sql)? + 1;
    let unticked_step_ids = if create.delete_unticked_plan {
        select_unticked_step_ids(sql, &create.item_id)?
    } else {
        Vec::new()
    };

    let grill = Grill {
        id: create.id,
        item_id: create.item_id.clone(),
        transcript: create.transcript,
        summary: create.summary,
        verdict: create.verdict,
        model_proposal: create.model_proposal,
        applied_patch: create.applied_patch,
        resulting_stage: resulting,
        completed_at: now_ms,
        version,
    };

    write_completion(sql, &grill, &item, resulting, &unticked_step_ids, now_ms, version)?;

    Ok(json(201, &grill))
}

/// The write burst a completed Grill applies. No transaction wraps this —
/// see the module doc for why none is available — so every statement here
/// runs only once every fallible check in [`create`] has already resolved.
/// The item row is only touched when the stage actually changes (same
/// "no settable field changed, no write" discipline as `items::patch`);
/// `unticked_step_ids` is already empty when `delete_unticked_plan` was
/// unset, so the loop below is a no-op in that case rather than a second
/// branch to keep in sync.
fn write_completion(
    sql: &dyn Sql,
    grill: &Grill,
    item: &Item,
    resulting_stage: hummingbird_domain::Stage,
    unticked_step_ids: &[String],
    now_ms: i64,
    version: i64,
) -> Result<(), SqlError> {
    sql.exec(
        "INSERT INTO grills (id, item_id, transcript, summary, verdict, model_proposal, \
         applied_patch, resulting_stage, completed_at, version) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        &grill_params(grill),
    )?;
    if resulting_stage != item.stage {
        sql.exec(
            "UPDATE items SET stage = ?, updated_at = ?, version = ? WHERE id = ?",
            &[
                SqlValue::Text(resulting_stage.as_str().to_string()),
                SqlValue::Integer(now_ms),
                SqlValue::Integer(version),
                SqlValue::Text(item.id.clone()),
            ],
        )?;
    }
    // Ticked (done=1) and already-deleted steps are never named here — the
    // caller only ever hands this a set of unticked, undeleted step ids —
    // so Record is untouched by construction, not merely by a predicate
    // that could be gotten wrong.
    for step_id in unticked_step_ids {
        sql.exec(
            "UPDATE steps SET deleted_at = ?, version = ? WHERE id = ?",
            &[
                SqlValue::Integer(now_ms),
                SqlValue::Integer(version),
                SqlValue::Text(step_id.clone()),
            ],
        )?;
    }
    write_meta_version(sql, version)?;
    Ok(())
}

/// Every currently-unticked, not-yet-deleted Step on an item — evaluated
/// fresh against live state at the moment the transaction opens, never
/// against a client-supplied snapshot, which is what makes a Step ticked or
/// deleted *during* the interview survive automatically: by the time this
/// runs, it is simply no longer in the set.
fn select_unticked_step_ids(sql: &dyn Sql, item_id: &str) -> Result<Vec<String>, SqlError> {
    let rows = sql.exec(
        "SELECT id FROM steps WHERE item_id = ? AND done = 0 AND deleted_at IS NULL",
        &[SqlValue::Text(item_id.to_string())],
    )?;
    rows.iter().map(|row| RowReader(row).text("id")).collect()
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

/// The target item, full row — [`create`] needs both its stage (for
/// [`resulting_stage`]) and its version (for the CAS check), so this reuses
/// `items::item_from_row` rather than a second, narrower reader.
fn select_item(sql: &dyn Sql, item_id: &str) -> Result<Option<Item>, SqlError> {
    let Some(row) = sql
        .exec(
            "SELECT * FROM items WHERE id = ?",
            &[SqlValue::Text(item_id.to_string())],
        )?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    Ok(Some(super::items::item_from_row(&row)?))
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
