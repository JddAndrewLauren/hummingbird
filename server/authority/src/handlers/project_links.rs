//! `POST /api/project_links` and `PATCH /api/project_links/:id` — one
//! ordered URL per project (#626, ADR-0030 decision 4), removed by flagging
//! `removed_at`, never deleted. Mirrors `fog.rs`'s shape exactly: the only
//! structural difference is the extra nullable `label` column and the
//! `url` field name.

use hummingbird_domain::{is_url_safe_id, CreateProjectLink, ProjectLink, ProjectLinkPatch};

use super::{conflict, error, json, parse_body, read_meta_version, write_meta_version, ApiResponse, ID_NOT_URL_SAFE};
use crate::codec::{RowReader, Sets};
use crate::sql::{Row, Sql, SqlError, SqlValue};

pub fn create(body: Option<&str>, _now_ms: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let create: CreateProjectLink = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    // Ahead of the replay select below, deliberately: an id outside the
    // charset can never be addressed as a path segment, so already-exists
    // must not answer 200 for one (#548).
    if !is_url_safe_id(&create.id) {
        return Ok(error(400, "validation", ID_NOT_URL_SAFE));
    }

    // Replay before the remaining validation: already-exists is success and
    // returns the stored row (ADR-0008), even under a divergent payload that
    // would no longer validate.
    if let Some(row) = select_link(sql, &create.id)? {
        return Ok(json(200, &link_from_row(&row)?));
    }

    if create.url.is_empty() {
        return Ok(error(400, "validation", "url must be non-empty"));
    }
    if !super::projects::project_exists(sql, &create.project_id)? {
        return Ok(error(400, "validation", "unknown project_id"));
    }

    let version = read_meta_version(sql)? + 1;
    let link = ProjectLink {
        id: create.id,
        project_id: create.project_id,
        url: create.url,
        label: create.label,
        position: create.position,
        removed_at: None,
        version,
    };
    sql.exec(
        "INSERT INTO project_links (id, project_id, url, label, position, removed_at, version) \
         VALUES (?, ?, ?, ?, ?, NULL, ?)",
        &[
            SqlValue::Text(link.id.clone()),
            SqlValue::Text(link.project_id.clone()),
            SqlValue::Text(link.url.clone()),
            SqlValue::from_opt_text(link.label.as_deref()),
            SqlValue::Integer(link.position),
            SqlValue::Integer(link.version),
        ],
    )?;
    write_meta_version(sql, version)?;
    Ok(json(201, &link))
}

pub fn patch(
    id: &str,
    body: Option<&str>,
    _now_ms: i64,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    let patch: ProjectLinkPatch = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if patch.url.as_deref() == Some("") {
        return Ok(error(400, "validation", "url must be non-empty"));
    }

    let Some(row) = select_link(sql, id)? else {
        return Ok(error(404, "not_found", "no such project link"));
    };
    let current = link_from_row(&row)?;
    if current.version != patch.expected_version {
        return Ok(conflict(&current));
    }

    // Compared typed against `current`, never a bare SQL-value `==` — see
    // items::patch for why.
    let mut sets = Sets::new();
    if let Some(url) = &patch.url {
        if *url != current.url {
            sets.set("url", SqlValue::Text(url.clone()));
        }
    }
    if let Some(label) = &patch.label {
        if *label != current.label {
            sets.set("label", SqlValue::from_opt_text(label.as_deref()));
        }
    }
    if let Some(position) = patch.position {
        if position != current.position {
            sets.set("position", SqlValue::Integer(position));
        }
    }
    if let Some(removed_at) = patch.removed_at {
        if removed_at != current.removed_at {
            sets.set("removed_at", SqlValue::from_opt_i64(removed_at));
        }
    }
    if sets.is_empty() {
        return Ok(json(200, &current));
    }

    let version = read_meta_version(sql)? + 1;
    sets.set("version", SqlValue::Integer(version));
    let update = sets.update_sql("project_links", "id = ?");
    let mut params = sets.into_params();
    params.push(SqlValue::Text(id.to_string()));
    sql.exec(&update, &params)?;
    write_meta_version(sql, version)?;

    let row = select_link(sql, id)?.ok_or_else(|| SqlError {
        message: "row vanished mid-update".into(),
    })?;
    Ok(json(200, &link_from_row(&row)?))
}

fn select_link(sql: &dyn Sql, id: &str) -> Result<Option<Row>, SqlError> {
    Ok(sql
        .exec(
            "SELECT * FROM project_links WHERE id = ?",
            &[SqlValue::Text(id.to_string())],
        )?
        .into_iter()
        .next())
}

/// Also the pull `changes.rs` uses for this table's sweep/delta rows.
pub(super) fn link_from_row(row: &Row) -> Result<ProjectLink, SqlError> {
    let r = RowReader(row);
    Ok(ProjectLink {
        id: r.text("id")?,
        project_id: r.text("project_id")?,
        url: r.text("url")?,
        label: r.opt_text("label"),
        position: r.int("position")?,
        removed_at: r.opt_int("removed_at"),
        version: r.int("version")?,
    })
}
