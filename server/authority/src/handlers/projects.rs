//! `POST /api/projects` and `PATCH /api/projects/:id`. A project's Route
//! row is born with it — the 1:1 invariant is structural, so there is no
//! route create anywhere.

use hummingbird_domain::{is_url_safe_id, CreateProject, Project, ProjectPatch};

use super::{conflict, error, json, parse_body, read_meta_version, write_meta_version, ApiResponse, ID_NOT_URL_SAFE};
use crate::codec::{RowReader, Sets};
use crate::sql::{Row, Sql, SqlError, SqlValue};

pub fn create(body: Option<&str>, now_ms: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let create: CreateProject = match parse_body(body) {
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
    if let Some(row) = select_project(sql, &create.id)? {
        return Ok(json(200, &project_from_row(&row)?));
    }

    if create.name.is_empty() {
        return Ok(error(400, "validation", "name must be non-empty"));
    }

    let version = read_meta_version(sql)? + 1;
    let project = Project {
        id: create.id,
        name: create.name,
        archived_at: None,
        created_at: now_ms,
        updated_at: now_ms,
        version,
    };
    sql.exec(
        "INSERT INTO projects (id, name, archived_at, created_at, updated_at, version) \
         VALUES (?, ?, NULL, ?, ?, ?)",
        &[
            SqlValue::Text(project.id.clone()),
            SqlValue::Text(project.name.clone()),
            SqlValue::Integer(project.created_at),
            SqlValue::Integer(project.updated_at),
            SqlValue::Integer(project.version),
        ],
    )?;
    // The Route row shares the project's version stamp: one HTTP write is
    // one counter bump, and the delta pull is `> ?`, so a shared stamp is
    // sound.
    sql.exec(
        "INSERT INTO routes (project_id, destination, notes, updated_at, version) \
         VALUES (?, NULL, NULL, ?, ?)",
        &[
            SqlValue::Text(project.id.clone()),
            SqlValue::Integer(now_ms),
            SqlValue::Integer(version),
        ],
    )?;
    write_meta_version(sql, version)?;
    Ok(json(201, &project))
}

pub fn patch(
    id: &str,
    body: Option<&str>,
    now_ms: i64,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    let patch: ProjectPatch = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if patch.name.as_deref() == Some("") {
        return Ok(error(400, "validation", "name must be non-empty"));
    }

    let Some(row) = select_project(sql, id)? else {
        return Ok(error(404, "not_found", "no such project"));
    };
    let current = project_from_row(&row)?;
    if current.version != patch.expected_version {
        return Ok(conflict(&current));
    }

    // Compared typed against `current`, never a bare SQL-value `==` — see
    // items::patch for why.
    let mut sets = Sets::new();
    if let Some(name) = &patch.name {
        if *name != current.name {
            sets.set("name", SqlValue::Text(name.clone()));
        }
    }
    if let Some(archived_at) = patch.archived_at {
        if archived_at != current.archived_at {
            sets.set("archived_at", SqlValue::from_opt_i64(archived_at));
        }
    }
    if sets.is_empty() {
        return Ok(json(200, &current));
    }

    let version = read_meta_version(sql)? + 1;
    sets.set("updated_at", SqlValue::Integer(now_ms));
    sets.set("version", SqlValue::Integer(version));
    let update = sets.update_sql("projects", "id = ?");
    let mut params = sets.into_params();
    params.push(SqlValue::Text(id.to_string()));
    sql.exec(&update, &params)?;
    write_meta_version(sql, version)?;

    let row = select_project(sql, id)?.ok_or_else(|| SqlError {
        message: "row vanished mid-update".into(),
    })?;
    Ok(json(200, &project_from_row(&row)?))
}

pub(super) fn project_exists(sql: &dyn Sql, id: &str) -> Result<bool, SqlError> {
    Ok(select_project(sql, id)?.is_some())
}

fn select_project(sql: &dyn Sql, id: &str) -> Result<Option<Row>, SqlError> {
    Ok(sql
        .exec(
            "SELECT * FROM projects WHERE id = ?",
            &[SqlValue::Text(id.to_string())],
        )?
        .into_iter()
        .next())
}

pub(super) fn project_from_row(row: &Row) -> Result<Project, SqlError> {
    let r = RowReader(row);
    Ok(Project {
        id: r.text("id")?,
        name: r.text("name")?,
        archived_at: r.opt_int("archived_at"),
        created_at: r.int("created_at")?,
        updated_at: r.int("updated_at")?,
        version: r.int("version")?,
    })
}
