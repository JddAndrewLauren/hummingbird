//! `POST /api/file_links` and `PATCH /api/file_links/:id` — the many
//! Dropbox-relative paths an item points at (ADR-0036), removed by flagging
//! `removed_at`, never deleted (ADR-0020). Mirrors `project_links.rs`'s
//! shape with two deliberate subtractions: no `label`/`position` columns,
//! and a patch that carries **only** `removed_at` — a file link is added and
//! removed whole, never re-pointed.
//!
//! `path` is validated non-blank after trim and nothing more, the same one
//! rule `items.rs` applies to `vault_path`: its shape (no leading `/`, no
//! `..`, no drive letter) is client-side vendor knowledge, and the authority
//! stores an opaque operator-chosen string.
//!
//! **Not part of the archive cascade.** `items::cascade_archive_for_project`
//! touches only `items`; file links are read through their item, exactly as
//! steps are, so an archived item's links go quiet with it and come back
//! with it.

use hummingbird_domain::{is_url_safe_id, CreateFileLink, FileLink, FileLinkPatch};

use super::{conflict, error, json, parse_body, read_meta_version, write_meta_version, ApiResponse, ID_NOT_URL_SAFE};
use crate::codec::{RowReader, Sets};
use crate::sql::{Row, Sql, SqlError, SqlValue};

pub fn create(body: Option<&str>, _now_ms: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let create: CreateFileLink = match parse_body(body) {
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

    if create.path.trim().is_empty() {
        return Ok(error(400, "validation", "path must be non-empty"));
    }
    if !super::items::item_exists(sql, &create.item_id)? {
        return Ok(error(400, "validation", "unknown item_id"));
    }

    let version = read_meta_version(sql)? + 1;
    let link = FileLink {
        id: create.id,
        item_id: create.item_id,
        path: create.path,
        removed_at: None,
        version,
    };
    sql.exec(
        "INSERT INTO file_links (id, item_id, path, removed_at, version) \
         VALUES (?, ?, ?, NULL, ?)",
        &[
            SqlValue::Text(link.id.clone()),
            SqlValue::Text(link.item_id.clone()),
            SqlValue::Text(link.path.clone()),
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
    let patch: FileLinkPatch = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };

    let Some(row) = select_link(sql, id)? else {
        return Ok(error(404, "not_found", "no such file link"));
    };
    let current = link_from_row(&row)?;
    if current.version != patch.expected_version {
        return Ok(conflict(&current));
    }

    // Compared typed against `current`, never a bare SQL-value `==` — see
    // items::patch for why.
    let mut sets = Sets::new();
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
    let update = sets.update_sql("file_links", "id = ?");
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
            "SELECT * FROM file_links WHERE id = ?",
            &[SqlValue::Text(id.to_string())],
        )?
        .into_iter()
        .next())
}

/// Also the pull `changes.rs` uses for this table's sweep/delta rows.
pub(super) fn link_from_row(row: &Row) -> Result<FileLink, SqlError> {
    let r = RowReader(row);
    Ok(FileLink {
        id: r.text("id")?,
        item_id: r.text("item_id")?,
        path: r.text("path")?,
        removed_at: r.opt_int("removed_at"),
        version: r.int("version")?,
    })
}
