//! `POST /api/steps` and `PATCH /api/steps/:id`. Ticking a Step —
//! `{expected_version, done: true}` — is the scalar CAS write ADR-0008
//! exists for. Deletion is flagged via `deleted_at`, never erased.

use hummingbird_domain::{is_url_safe_id, CreateStep, Step, StepPatch};

use super::{conflict, error, json, parse_body, read_meta_version, write_meta_version, ApiResponse, ID_NOT_URL_SAFE};
use crate::codec::{RowReader, Sets};
use crate::sql::{Row, Sql, SqlError, SqlValue};

pub fn create(body: Option<&str>, _now_ms: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let create: CreateStep = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    // Ahead of the replay select below, deliberately: an id outside
    // the charset can never be addressed as a path segment, so
    // already-exists must not answer 200 for one (#548). Empty is
    // one of the shapes this rejects.
    if !is_url_safe_id(&create.id) {
        return Ok(error(400, "validation", ID_NOT_URL_SAFE));
    }

    // Replay before the remaining validation: already-exists is success and
    // returns the stored row (ADR-0008), even under a divergent payload that
    // would no longer validate.
    if let Some(row) = select_step(sql, &create.id)? {
        return Ok(json(200, &step_from_row(&row)?));
    }

    if create.body.is_empty() {
        return Ok(error(400, "validation", "body must be non-empty"));
    }
    if !super::items::item_exists(sql, &create.item_id)? {
        return Ok(error(400, "validation", "unknown item_id"));
    }

    let version = read_meta_version(sql)? + 1;
    let step = Step {
        id: create.id,
        item_id: create.item_id,
        body: create.body,
        done: false,
        position: create.position,
        deleted_at: None,
        version,
    };
    sql.exec(
        "INSERT INTO steps (id, item_id, body, done, position, deleted_at, version) \
         VALUES (?, ?, ?, 0, ?, NULL, ?)",
        &[
            SqlValue::Text(step.id.clone()),
            SqlValue::Text(step.item_id.clone()),
            SqlValue::Text(step.body.clone()),
            SqlValue::Integer(step.position),
            SqlValue::Integer(step.version),
        ],
    )?;
    write_meta_version(sql, version)?;
    Ok(json(201, &step))
}

pub fn patch(
    id: &str,
    body: Option<&str>,
    _now_ms: i64,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    let patch: StepPatch = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if patch.body.as_deref() == Some("") {
        return Ok(error(400, "validation", "body must be non-empty"));
    }

    let Some(row) = select_step(sql, id)? else {
        return Ok(error(404, "not_found", "no such step"));
    };
    let current = step_from_row(&row)?;
    if current.version != patch.expected_version {
        return Ok(conflict(&current));
    }

    // Compared typed against `current`, never a bare SQL-value `==` — see
    // items::patch for why.
    let mut sets = Sets::new();
    if let Some(step_body) = &patch.body {
        if *step_body != current.body {
            sets.set("body", SqlValue::Text(step_body.clone()));
        }
    }
    if let Some(done) = patch.done {
        if done != current.done {
            sets.set("done", SqlValue::Integer(done as i64));
        }
    }
    if let Some(position) = patch.position {
        if position != current.position {
            sets.set("position", SqlValue::Integer(position));
        }
    }
    if let Some(deleted_at) = patch.deleted_at {
        if deleted_at != current.deleted_at {
            sets.set("deleted_at", SqlValue::from_opt_i64(deleted_at));
        }
    }
    if sets.is_empty() {
        return Ok(json(200, &current));
    }

    let version = read_meta_version(sql)? + 1;
    sets.set("version", SqlValue::Integer(version));
    let update = sets.update_sql("steps", "id = ?");
    let mut params = sets.into_params();
    params.push(SqlValue::Text(id.to_string()));
    sql.exec(&update, &params)?;
    write_meta_version(sql, version)?;

    let row = select_step(sql, id)?.ok_or_else(|| SqlError {
        message: "row vanished mid-update".into(),
    })?;
    Ok(json(200, &step_from_row(&row)?))
}

fn select_step(sql: &dyn Sql, id: &str) -> Result<Option<Row>, SqlError> {
    Ok(sql
        .exec(
            "SELECT * FROM steps WHERE id = ?",
            &[SqlValue::Text(id.to_string())],
        )?
        .into_iter()
        .next())
}

pub(super) fn step_from_row(row: &Row) -> Result<Step, SqlError> {
    let r = RowReader(row);
    Ok(Step {
        id: r.text("id")?,
        item_id: r.text("item_id")?,
        body: r.text("body")?,
        done: r.bool_int("done")?,
        position: r.int("position")?,
        deleted_at: r.opt_int("deleted_at"),
        version: r.int("version")?,
    })
}
