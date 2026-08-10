//! `PATCH /api/routes/:project_id` — the only route write. The row itself
//! is created by project create (see `projects.rs`); `/to-actions` owns the
//! content.

use hummingbird_domain::{Route, RoutePatch};

use super::{conflict, error, json, parse_body, read_meta_version, write_meta_version, ApiResponse};
use crate::codec::{RowReader, Sets};
use crate::sql::{Row, Sql, SqlError, SqlValue};

pub fn patch(
    project_id: &str,
    body: Option<&str>,
    now_ms: i64,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    let patch: RoutePatch = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };

    let Some(row) = select_route(sql, project_id)? else {
        return Ok(error(404, "not_found", "no such route"));
    };
    let current = route_from_row(&row)?;
    if current.version != patch.expected_version {
        return Ok(conflict(&current));
    }

    // Compared typed against `current`, never a bare SQL-value `==` — see
    // items::patch for why.
    let mut sets = Sets::new();
    if let Some(destination) = &patch.destination {
        if *destination != current.destination {
            sets.set("destination", SqlValue::from_opt_text(destination.as_deref()));
        }
    }
    if let Some(notes) = &patch.notes {
        if *notes != current.notes {
            sets.set("notes", SqlValue::from_opt_text(notes.as_deref()));
        }
    }
    if sets.is_empty() {
        return Ok(json(200, &current));
    }

    let version = read_meta_version(sql)? + 1;
    sets.set("updated_at", SqlValue::Integer(now_ms));
    sets.set("version", SqlValue::Integer(version));
    let update = sets.update_sql("routes", "project_id = ?");
    let mut params = sets.into_params();
    params.push(SqlValue::Text(project_id.to_string()));
    sql.exec(&update, &params)?;
    write_meta_version(sql, version)?;

    let row = select_route(sql, project_id)?.ok_or_else(|| SqlError {
        message: "row vanished mid-update".into(),
    })?;
    Ok(json(200, &route_from_row(&row)?))
}

fn select_route(sql: &dyn Sql, project_id: &str) -> Result<Option<Row>, SqlError> {
    Ok(sql
        .exec(
            "SELECT * FROM routes WHERE project_id = ?",
            &[SqlValue::Text(project_id.to_string())],
        )?
        .into_iter()
        .next())
}

pub(super) fn route_from_row(row: &Row) -> Result<Route, SqlError> {
    let r = RowReader(row);
    Ok(Route {
        project_id: r.text("project_id")?,
        destination: r.opt_text("destination"),
        notes: r.opt_text("notes"),
        updated_at: r.int("updated_at")?,
        version: r.int("version")?,
    })
}
