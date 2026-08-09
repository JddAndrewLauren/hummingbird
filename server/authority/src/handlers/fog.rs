//! `POST /api/fog` and `PATCH /api/fog/:id` — Fog segments, resolved by
//! flagging `resolved_at`, never deleted.

use hummingbird_domain::{CreateFog, Fog, FogPatch};

use super::{conflict, error, json, parse_body, read_meta_version, write_meta_version, ApiResponse};
use crate::codec::{RowReader, Sets};
use crate::sql::{Row, Sql, SqlError, SqlValue};

pub fn create(body: Option<&str>, _now_ms: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let create: CreateFog = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if create.id.is_empty() {
        return Ok(error(400, "validation", "id must be non-empty"));
    }

    // Replay before the remaining validation: already-exists is success and
    // returns the stored row (ADR-0008), even under a divergent payload that
    // would no longer validate.
    if let Some(row) = select_fog(sql, &create.id)? {
        return Ok(json(200, &fog_from_row(&row)?));
    }

    if create.question.is_empty() {
        return Ok(error(400, "validation", "question must be non-empty"));
    }
    if !super::projects::project_exists(sql, &create.project_id)? {
        return Ok(error(400, "validation", "unknown project_id"));
    }

    let version = read_meta_version(sql)? + 1;
    let fog = Fog {
        id: create.id,
        project_id: create.project_id,
        question: create.question,
        position: create.position,
        resolved_at: None,
        version,
    };
    sql.exec(
        "INSERT INTO fog (id, project_id, question, position, resolved_at, version) \
         VALUES (?, ?, ?, ?, NULL, ?)",
        &[
            SqlValue::Text(fog.id.clone()),
            SqlValue::Text(fog.project_id.clone()),
            SqlValue::Text(fog.question.clone()),
            SqlValue::Integer(fog.position),
            SqlValue::Integer(fog.version),
        ],
    )?;
    write_meta_version(sql, version)?;
    Ok(json(201, &fog))
}

pub fn patch(
    id: &str,
    body: Option<&str>,
    _now_ms: i64,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    let patch: FogPatch = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if patch.question.as_deref() == Some("") {
        return Ok(error(400, "validation", "question must be non-empty"));
    }

    let Some(row) = select_fog(sql, id)? else {
        return Ok(error(404, "not_found", "no such fog segment"));
    };
    let current = fog_from_row(&row)?;
    if current.version != patch.expected_version {
        return Ok(conflict(&current));
    }

    let mut sets = Sets::new();
    if let Some(question) = &patch.question {
        sets.set("question", SqlValue::Text(question.clone()));
    }
    if let Some(position) = patch.position {
        sets.set("position", SqlValue::Integer(position));
    }
    if let Some(resolved_at) = patch.resolved_at {
        sets.set("resolved_at", SqlValue::from_opt_i64(resolved_at));
    }
    if sets.is_empty() {
        return Ok(json(200, &current));
    }

    let version = read_meta_version(sql)? + 1;
    sets.set("version", SqlValue::Integer(version));
    let update = sets.update_sql("fog", "id = ?");
    let mut params = sets.into_params();
    params.push(SqlValue::Text(id.to_string()));
    sql.exec(&update, &params)?;
    write_meta_version(sql, version)?;

    let row = select_fog(sql, id)?.ok_or_else(|| SqlError {
        message: "row vanished mid-update".into(),
    })?;
    Ok(json(200, &fog_from_row(&row)?))
}

fn select_fog(sql: &dyn Sql, id: &str) -> Result<Option<Row>, SqlError> {
    Ok(sql
        .exec(
            "SELECT * FROM fog WHERE id = ?",
            &[SqlValue::Text(id.to_string())],
        )?
        .into_iter()
        .next())
}

pub(super) fn fog_from_row(row: &Row) -> Result<Fog, SqlError> {
    let r = RowReader(row);
    Ok(Fog {
        id: r.text("id")?,
        project_id: r.text("project_id")?,
        question: r.text("question")?,
        position: r.int("position")?,
        resolved_at: r.opt_int("resolved_at"),
        version: r.int("version")?,
    })
}
