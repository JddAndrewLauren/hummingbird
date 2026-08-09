//! Routing, parsing, validation and the three S0 routes (#113). Everything
//! here is a pure function of the request, the injected clock and the
//! [`Sql`] seam — the shim adds nothing but transport.

use hummingbird_domain::{
    ApiError, ChangesResponse, ConflictResponse, CreateItem, Energy, Item, ItemPatch, Size, Stage,
    VERSION_CONFLICT,
};
use serde::Serialize;

use crate::sql::{Row, Sql, SqlError, SqlValue};

/// The transport-agnostic request: the shim maps `worker::Request` onto
/// this, tests build it directly.
pub struct ApiRequest<'a> {
    pub method: &'a str,
    /// Path only, no query string — e.g. `/api/items/uuid-1`.
    pub path: &'a str,
    /// The raw query string, without the `?`.
    pub query: Option<&'a str>,
    pub body: Option<&'a str>,
}

/// Status + JSON body; every response body is JSON.
#[derive(Debug, PartialEq)]
pub struct ApiResponse {
    pub status: u16,
    pub body: String,
}

/// The one entry point. `now_ms` is injected (the shim passes the worker
/// clock) — nothing in this crate reads a clock of its own.
pub fn handle(req: &ApiRequest, now_ms: i64, sql: &dyn Sql) -> ApiResponse {
    let result = match (req.method, req.path) {
        ("POST", "/api/items") => create_item(req.body, now_ms, sql),
        (_, "/api/items") => Ok(method_not_allowed()),
        ("PATCH", path) if path.strip_prefix("/api/items/").is_some_and(|id| !id.is_empty()) => {
            let id = path.strip_prefix("/api/items/").expect("guard matched");
            patch_item(id, req.body, now_ms, sql)
        }
        (_, path) if path.strip_prefix("/api/items/").is_some_and(|id| !id.is_empty()) => {
            Ok(method_not_allowed())
        }
        ("GET", "/api/changes") => changes(req.query, sql),
        (_, "/api/changes") => Ok(method_not_allowed()),
        _ => Ok(error(404, "not_found", "no such route")),
    };
    result.unwrap_or_else(|e| error(500, "internal", &e.message))
}

// ---------------------------------------------------------------- routes

fn create_item(body: Option<&str>, now_ms: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let create: CreateItem = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if create.id.is_empty() {
        return Ok(error(400, "validation", "id must be non-empty"));
    }
    if create.title.is_empty() {
        return Ok(error(400, "validation", "title must be non-empty"));
    }
    let priority = create.priority.unwrap_or(0);
    if !(0..=4).contains(&priority) {
        return Ok(error(400, "validation", "priority must be between 0 and 4"));
    }

    // Idempotent by client-supplied id: a replay is answered with the
    // current row, no write, no version bump (ADR-0008) — even a divergent
    // payload on the same id returns the stored row unchanged. The DO is
    // single-threaded, so SELECT-then-INSERT cannot race.
    if let Some(row) = select_item(sql, &create.id)? {
        return Ok(json(200, &item_from_row(&row)?));
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
        due_date: create.due_date,
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
         priority, project_id, project_pos, due_date, scheduled_date, source, source_key, \
         source_url, archived_at, created_at, updated_at, version) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        &item_params(&item),
    )?;
    write_meta_version(sql, version)?;
    Ok(json(201, &item))
}

fn patch_item(
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

    let Some(row) = select_item(sql, id)? else {
        return Ok(error(404, "not_found", "no such item"));
    };
    let current = item_from_row(&row)?;
    if current.version != patch.expected_version {
        // The 409 carries the current entity so the client can rebase
        // (ADR-0008): disjoint touched fields auto-resend, same-field loses
        // into the client-side dead-letter journal.
        return Ok(json(
            409,
            &ConflictResponse {
                error: VERSION_CONFLICT.to_string(),
                current,
            },
        ));
    }

    // Absolute-value sets only: each touched field's entire new value.
    let mut sets: Vec<&str> = Vec::new();
    let mut params: Vec<SqlValue> = Vec::new();
    if let Some(title) = &patch.title {
        sets.push("title = ?");
        params.push(SqlValue::Text(title.clone()));
    }
    if let Some(description) = &patch.description {
        sets.push("description = ?");
        params.push(SqlValue::from_opt_text(description.as_deref()));
    }
    if let Some(stage) = patch.stage {
        sets.push("stage = ?");
        params.push(SqlValue::Text(stage.as_str().to_string()));
    }
    if let Some(size) = patch.size {
        sets.push("size = ?");
        params.push(SqlValue::from_opt_text(size.map(Size::as_str)));
    }
    if let Some(energy) = patch.energy {
        sets.push("energy = ?");
        params.push(SqlValue::from_opt_text(energy.map(Energy::as_str)));
    }
    if let Some(context) = &patch.context {
        sets.push("context = ?");
        params.push(SqlValue::from_opt_text(context.as_deref()));
    }
    if let Some(priority) = patch.priority {
        sets.push("priority = ?");
        params.push(SqlValue::Integer(priority));
    }
    if let Some(project_id) = &patch.project_id {
        sets.push("project_id = ?");
        params.push(SqlValue::from_opt_text(project_id.as_deref()));
    }
    if let Some(project_pos) = patch.project_pos {
        sets.push("project_pos = ?");
        params.push(SqlValue::from_opt_i64(project_pos));
    }
    if let Some(due_date) = &patch.due_date {
        sets.push("due_date = ?");
        params.push(SqlValue::from_opt_text(due_date.as_deref()));
    }
    if let Some(scheduled_date) = &patch.scheduled_date {
        sets.push("scheduled_date = ?");
        params.push(SqlValue::from_opt_text(scheduled_date.as_deref()));
    }
    if let Some(archived_at) = patch.archived_at {
        sets.push("archived_at = ?");
        params.push(SqlValue::from_opt_i64(archived_at));
    }

    // No settable field touched: answer with the current row, no UPDATE —
    // a version bump here would force every peer to re-pull an unchanged row.
    if sets.is_empty() {
        return Ok(json(200, &current));
    }

    let version = read_meta_version(sql)? + 1;
    sets.push("updated_at = ?");
    params.push(SqlValue::Integer(now_ms));
    sets.push("version = ?");
    params.push(SqlValue::Integer(version));
    params.push(SqlValue::Text(id.to_string()));
    sql.exec(
        &format!("UPDATE items SET {} WHERE id = ?", sets.join(", ")),
        &params,
    )?;
    write_meta_version(sql, version)?;

    let row = select_item(sql, id)?.ok_or_else(|| SqlError {
        message: "row vanished mid-update".into(),
    })?;
    Ok(json(200, &item_from_row(&row)?))
}

fn changes(query: Option<&str>, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let Some(since) = query_param(query, "since").and_then(|v| v.parse::<i64>().ok()) else {
        return Ok(error(400, "validation", "since must be an integer"));
    };

    // The gate: one meta row read answers "anything new?" — the unchanged
    // workspace never touches `items` (ADR-0008's rows-read argument).
    let version = read_meta_version(sql)?;
    if since >= version {
        return Ok(json(200, &ChangesResponse { version, items: vec![] }));
    }

    let rows = sql.exec(
        "SELECT * FROM items WHERE version > ? ORDER BY version",
        &[SqlValue::Integer(since)],
    )?;
    let items = rows.iter().map(item_from_row).collect::<Result<Vec<_>, _>>()?;
    Ok(json(200, &ChangesResponse { version, items }))
}

// --------------------------------------------------------------- helpers

fn parse_body<T: serde::de::DeserializeOwned>(body: Option<&str>) -> Result<T, ApiResponse> {
    let body = body.filter(|b| !b.is_empty()).ok_or_else(|| {
        error(400, "bad_json", "a JSON body is required")
    })?;
    serde_json::from_str(body).map_err(|e| error(400, "bad_json", &e.to_string()))
}

fn query_param<'a>(query: Option<&'a str>, name: &str) -> Option<&'a str> {
    query?
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
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

fn read_meta_version(sql: &dyn Sql) -> Result<i64, SqlError> {
    sql.exec("SELECT version FROM meta WHERE id = 1", &[])?
        .first()
        .and_then(|r| r.get("version").and_then(SqlValue::as_i64))
        .ok_or_else(|| SqlError {
            message: "meta row missing — init_schema not run".into(),
        })
}

fn write_meta_version(sql: &dyn Sql, version: i64) -> Result<(), SqlError> {
    sql.exec(
        "UPDATE meta SET version = ? WHERE id = 1",
        &[SqlValue::Integer(version)],
    )?;
    Ok(())
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
        SqlValue::from_opt_text(item.due_date.as_deref()),
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

fn item_from_row(row: &Row) -> Result<Item, SqlError> {
    let text = |col: &str| -> Result<String, SqlError> {
        row.get(col)
            .and_then(|v| v.as_text().map(str::to_string))
            .ok_or_else(|| bad_cell(col))
    };
    let opt_text =
        |col: &str| -> Option<String> { row.get(col).and_then(|v| v.as_text().map(str::to_string)) };
    let int = |col: &str| -> Result<i64, SqlError> {
        row.get(col).and_then(SqlValue::as_i64).ok_or_else(|| bad_cell(col))
    };
    let opt_int = |col: &str| -> Option<i64> { row.get(col).and_then(SqlValue::as_i64) };

    let stage_text = text("stage")?;
    Ok(Item {
        id: text("id")?,
        seq: opt_int("seq"),
        title: text("title")?,
        description: opt_text("description"),
        stage: Stage::parse(&stage_text).ok_or_else(|| bad_cell("stage"))?,
        size: opt_text("size").as_deref().and_then(Size::parse),
        energy: opt_text("energy").as_deref().and_then(Energy::parse),
        context: opt_text("context"),
        priority: int("priority")?,
        project_id: opt_text("project_id"),
        project_pos: opt_int("project_pos"),
        due_date: opt_text("due_date"),
        scheduled_date: opt_text("scheduled_date"),
        source: opt_text("source"),
        source_key: opt_text("source_key"),
        source_url: opt_text("source_url"),
        archived_at: opt_int("archived_at"),
        created_at: int("created_at")?,
        updated_at: int("updated_at")?,
        version: int("version")?,
    })
}

fn bad_cell(col: &str) -> SqlError {
    SqlError {
        message: format!("column `{col}` missing or mistyped"),
    }
}

fn json<T: Serialize>(status: u16, value: &T) -> ApiResponse {
    ApiResponse {
        status,
        body: serde_json::to_string(value).expect("DTOs serialize"),
    }
}

fn error(status: u16, code: &str, message: &str) -> ApiResponse {
    json(
        status,
        &ApiError {
            error: code.to_string(),
            message: message.to_string(),
        },
    )
}

fn method_not_allowed() -> ApiResponse {
    error(405, "method_not_allowed", "wrong method for this route")
}
