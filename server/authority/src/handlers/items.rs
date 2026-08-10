//! `POST /api/items` and `PATCH /api/items/:id` — the S0 routes, on the
//! shared codec.

use hummingbird_domain::{is_valid_deadline, CreateItem, Energy, Item, ItemPatch, Size, Stage};

use super::{
    conflict, error, json, parse_body, read_meta_version, write_meta_version, ApiResponse,
};
use crate::codec::{bad_cell, RowReader, Sets};
use crate::sql::{Row, Sql, SqlError, SqlValue};

pub fn create(body: Option<&str>, now_ms: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let create: CreateItem = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if create.id.is_empty() {
        return Ok(error(400, "validation", "id must be non-empty"));
    }

    // Idempotent by client-supplied id: a replay is answered with the
    // current row, no write, no version bump (ADR-0008) — even a divergent
    // payload on the same id returns the stored row unchanged, which is why
    // the select runs before the remaining validation: already-exists is
    // success, never a 400. The DO is single-threaded, so SELECT-then-INSERT
    // cannot race.
    if let Some(row) = select_item(sql, &create.id)? {
        return Ok(json(200, &item_from_row(&row)?));
    }

    if create.title.is_empty() {
        return Ok(error(400, "validation", "title must be non-empty"));
    }
    let priority = create.priority.unwrap_or(0);
    if !(0..=4).contains(&priority) {
        return Ok(error(400, "validation", "priority must be between 0 and 4"));
    }
    if let Some(project_id) = &create.project_id {
        if !super::projects::project_exists(sql, project_id)? {
            return Ok(error(400, "validation", "unknown project_id"));
        }
    }
    if let Some(deadline) = &create.deadline {
        if !is_valid_deadline(deadline) {
            return Ok(error(400, "validation", "deadline must be YYYY-MM-DD or YYYY-MM-DDTHH:MM"));
        }
    }

    let version = read_meta_version(sql)? + 1;
    let seq = sql
        .exec("SELECT COALESCE(MAX(seq), 0) + 1 AS next FROM items", &[])?
        .first()
        .and_then(|r| r.get("next").and_then(SqlValue::as_i64))
        .ok_or_else(|| SqlError {
            message: "seq mint returned no row".into(),
        })?;

    let item = Item {
        id: create.id,
        seq: Some(seq),
        title: create.title,
        description: create.description,
        stage: create.stage.unwrap_or(Stage::Triage),
        size: create.size,
        energy: create.energy,
        context: create.context,
        priority,
        project_id: create.project_id,
        project_pos: create.project_pos,
        deadline: create.deadline,
        scheduled_date: create.scheduled_date,
        source: create.source,
        source_key: create.source_key,
        source_url: create.source_url,
        archived_at: None,
        created_at: now_ms,
        updated_at: now_ms,
        version,
    };
    sql.exec(
        "INSERT INTO items (id, seq, title, description, stage, size, energy, context, \
         priority, project_id, project_pos, deadline, scheduled_date, source, source_key, \
         source_url, archived_at, created_at, updated_at, version) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        &item_params(&item),
    )?;
    write_meta_version(sql, version)?;
    Ok(json(201, &item))
}

pub fn patch(
    id: &str,
    body: Option<&str>,
    now_ms: i64,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    let patch: ItemPatch = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if patch.title.as_deref() == Some("") {
        return Ok(error(400, "validation", "title must be non-empty"));
    }
    if patch.priority.is_some_and(|p| !(0..=4).contains(&p)) {
        return Ok(error(400, "validation", "priority must be between 0 and 4"));
    }
    if let Some(Some(project_id)) = &patch.project_id {
        if !super::projects::project_exists(sql, project_id)? {
            return Ok(error(400, "validation", "unknown project_id"));
        }
    }
    if let Some(Some(deadline)) = &patch.deadline {
        if !is_valid_deadline(deadline) {
            return Ok(error(400, "validation", "deadline must be YYYY-MM-DD or YYYY-MM-DDTHH:MM"));
        }
    }

    let Some(row) = select_item(sql, id)? else {
        return Ok(error(404, "not_found", "no such item"));
    };
    let current = item_from_row(&row)?;
    if current.version != patch.expected_version {
        // Disjoint touched fields auto-resend against the carried entity;
        // same-field loses into the client-side dead-letter journal.
        return Ok(conflict(&current));
    }

    // Absolute-value sets only: each touched field's entire new value.
    // Compared typed against `current` (already the deserialized row) —
    // never a bare SQL-value `==`, which the integer/real hazard on a DO
    // cursor would make silently unequal on `f64` vs `Integer` (see the
    // sql module doc for `SqlValue::as_i64`). A field whose submitted value
    // already matches the stored one is left out of `sets` entirely, same
    // as an untouched field.
    let mut sets = Sets::new();
    if let Some(title) = &patch.title {
        if *title != current.title {
            sets.set("title", SqlValue::Text(title.clone()));
        }
    }
    if let Some(description) = &patch.description {
        if *description != current.description {
            sets.set("description", SqlValue::from_opt_text(description.as_deref()));
        }
    }
    if let Some(stage) = patch.stage {
        if stage != current.stage {
            sets.set("stage", SqlValue::Text(stage.as_str().to_string()));
        }
    }
    if let Some(size) = patch.size {
        if size != current.size {
            sets.set("size", SqlValue::from_opt_text(size.map(Size::as_str)));
        }
    }
    if let Some(energy) = patch.energy {
        if energy != current.energy {
            sets.set("energy", SqlValue::from_opt_text(energy.map(Energy::as_str)));
        }
    }
    if let Some(context) = &patch.context {
        if *context != current.context {
            sets.set("context", SqlValue::from_opt_text(context.as_deref()));
        }
    }
    if let Some(priority) = patch.priority {
        if priority != current.priority {
            sets.set("priority", SqlValue::Integer(priority));
        }
    }
    if let Some(project_id) = &patch.project_id {
        if *project_id != current.project_id {
            sets.set("project_id", SqlValue::from_opt_text(project_id.as_deref()));
        }
    }
    if let Some(project_pos) = patch.project_pos {
        if project_pos != current.project_pos {
            sets.set("project_pos", SqlValue::from_opt_i64(project_pos));
        }
    }
    if let Some(deadline) = &patch.deadline {
        if *deadline != current.deadline {
            sets.set("deadline", SqlValue::from_opt_text(deadline.as_deref()));
        }
    }
    if let Some(scheduled_date) = &patch.scheduled_date {
        if *scheduled_date != current.scheduled_date {
            sets.set("scheduled_date", SqlValue::from_opt_text(scheduled_date.as_deref()));
        }
    }
    if let Some(archived_at) = patch.archived_at {
        if archived_at != current.archived_at {
            sets.set("archived_at", SqlValue::from_opt_i64(archived_at));
        }
    }

    // No settable field actually changes the stored value: answer with the
    // current row, no UPDATE — a version bump here would force every peer
    // to re-pull an unchanged row.
    if sets.is_empty() {
        return Ok(json(200, &current));
    }

    let version = read_meta_version(sql)? + 1;
    sets.set("updated_at", SqlValue::Integer(now_ms));
    sets.set("version", SqlValue::Integer(version));
    let update = sets.update_sql("items", "id = ?");
    let mut params = sets.into_params();
    params.push(SqlValue::Text(id.to_string()));
    sql.exec(&update, &params)?;
    write_meta_version(sql, version)?;

    let row = select_item(sql, id)?.ok_or_else(|| SqlError {
        message: "row vanished mid-update".into(),
    })?;
    Ok(json(200, &item_from_row(&row)?))
}

pub(super) fn item_exists(sql: &dyn Sql, id: &str) -> Result<bool, SqlError> {
    Ok(select_item(sql, id)?.is_some())
}

fn select_item(sql: &dyn Sql, id: &str) -> Result<Option<Row>, SqlError> {
    Ok(sql
        .exec(
            "SELECT * FROM items WHERE id = ?",
            &[SqlValue::Text(id.to_string())],
        )?
        .into_iter()
        .next())
}

/// The INSERT's parameter list, in exactly its column order.
fn item_params(item: &Item) -> Vec<SqlValue> {
    vec![
        SqlValue::Text(item.id.clone()),
        SqlValue::from_opt_i64(item.seq),
        SqlValue::Text(item.title.clone()),
        SqlValue::from_opt_text(item.description.as_deref()),
        SqlValue::Text(item.stage.as_str().to_string()),
        SqlValue::from_opt_text(item.size.map(Size::as_str)),
        SqlValue::from_opt_text(item.energy.map(Energy::as_str)),
        SqlValue::from_opt_text(item.context.as_deref()),
        SqlValue::Integer(item.priority),
        SqlValue::from_opt_text(item.project_id.as_deref()),
        SqlValue::from_opt_i64(item.project_pos),
        SqlValue::from_opt_text(item.deadline.as_deref()),
        SqlValue::from_opt_text(item.scheduled_date.as_deref()),
        SqlValue::from_opt_text(item.source.as_deref()),
        SqlValue::from_opt_text(item.source_key.as_deref()),
        SqlValue::from_opt_text(item.source_url.as_deref()),
        SqlValue::from_opt_i64(item.archived_at),
        SqlValue::Integer(item.created_at),
        SqlValue::Integer(item.updated_at),
        SqlValue::Integer(item.version),
    ]
}

pub(super) fn item_from_row(row: &Row) -> Result<Item, SqlError> {
    let r = RowReader(row);
    let stage_text = r.text("stage")?;
    Ok(Item {
        id: r.text("id")?,
        seq: r.opt_int("seq"),
        title: r.text("title")?,
        description: r.opt_text("description"),
        stage: Stage::parse(&stage_text).ok_or_else(|| bad_cell("stage"))?,
        size: r.opt_text("size").as_deref().and_then(Size::parse),
        energy: r.opt_text("energy").as_deref().and_then(Energy::parse),
        context: r.opt_text("context"),
        priority: r.int("priority")?,
        project_id: r.opt_text("project_id"),
        project_pos: r.opt_int("project_pos"),
        deadline: r.opt_text("deadline"),
        scheduled_date: r.opt_text("scheduled_date"),
        source: r.opt_text("source"),
        source_key: r.opt_text("source_key"),
        source_url: r.opt_text("source_url"),
        archived_at: r.opt_int("archived_at"),
        created_at: r.int("created_at")?,
        updated_at: r.int("updated_at")?,
        version: r.int("version")?,
    })
}
