//! `POST /api/blocked_by` and `PATCH /api/blocked_by/:item_id/:blocker_id`.
//! The edge's identity is the composite key itself. Removal flags
//! `removed_at` under CAS; a re-add (POST on a removed edge) clears the
//! flag on the same row — a real write. A crash-replay of either lands on
//! the state it wanted and no-ops.

use hummingbird_domain::{BlockedBy, CreateBlockedBy, BlockedByPatch};

use super::{conflict, error, json, parse_body, read_meta_version, write_meta_version, ApiResponse};
use crate::codec::{RowReader, Sets};
use crate::sql::{Row, Sql, SqlError, SqlValue};

pub fn create(body: Option<&str>, _now_ms: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let create: CreateBlockedBy = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if create.item_id.is_empty() || create.blocker_id.is_empty() {
        return Ok(error(400, "validation", "item_id and blocker_id must be non-empty"));
    }
    if create.item_id == create.blocker_id {
        return Ok(error(400, "validation", "an item cannot block itself"));
    }
    for (field, id) in [("item_id", &create.item_id), ("blocker_id", &create.blocker_id)] {
        if !super::items::item_exists(sql, id)? {
            return Ok(error(400, "validation", &format!("unknown {field}")));
        }
    }

    if let Some(row) = select_edge(sql, &create.item_id, &create.blocker_id)? {
        let current = edge_from_row(&row)?;
        if current.removed_at.is_none() {
            // Live edge: the create is a replay, answered with the stored
            // row — no write, no bump.
            return Ok(json(200, &current));
        }
        // Removed edge: the POST re-adds it — a real write on the same row.
        let version = read_meta_version(sql)? + 1;
        sql.exec(
            "UPDATE blocked_by SET removed_at = NULL, version = ? \
             WHERE item_id = ? AND blocker_id = ?",
            &[
                SqlValue::Integer(version),
                SqlValue::Text(create.item_id.clone()),
                SqlValue::Text(create.blocker_id.clone()),
            ],
        )?;
        write_meta_version(sql, version)?;
        return Ok(json(
            200,
            &BlockedBy {
                removed_at: None,
                version,
                ..current
            },
        ));
    }

    let version = read_meta_version(sql)? + 1;
    let edge = BlockedBy {
        item_id: create.item_id,
        blocker_id: create.blocker_id,
        version,
        removed_at: None,
    };
    sql.exec(
        "INSERT INTO blocked_by (item_id, blocker_id, version, removed_at) \
         VALUES (?, ?, ?, NULL)",
        &[
            SqlValue::Text(edge.item_id.clone()),
            SqlValue::Text(edge.blocker_id.clone()),
            SqlValue::Integer(edge.version),
        ],
    )?;
    write_meta_version(sql, version)?;
    Ok(json(201, &edge))
}

pub fn patch(
    item_id: &str,
    blocker_id: &str,
    body: Option<&str>,
    _now_ms: i64,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    let patch: BlockedByPatch = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };

    let Some(row) = select_edge(sql, item_id, blocker_id)? else {
        return Ok(error(404, "not_found", "no such edge"));
    };
    let current = edge_from_row(&row)?;
    if current.version != patch.expected_version {
        return Ok(conflict(&current));
    }

    let mut sets = Sets::new();
    if let Some(removed_at) = patch.removed_at {
        sets.set("removed_at", SqlValue::from_opt_i64(removed_at));
    }
    if sets.is_empty() {
        return Ok(json(200, &current));
    }

    let version = read_meta_version(sql)? + 1;
    sets.set("version", SqlValue::Integer(version));
    let update = sets.update_sql("blocked_by", "item_id = ? AND blocker_id = ?");
    let mut params = sets.into_params();
    params.push(SqlValue::Text(item_id.to_string()));
    params.push(SqlValue::Text(blocker_id.to_string()));
    sql.exec(&update, &params)?;
    write_meta_version(sql, version)?;

    let row = select_edge(sql, item_id, blocker_id)?.ok_or_else(|| SqlError {
        message: "row vanished mid-update".into(),
    })?;
    Ok(json(200, &edge_from_row(&row)?))
}

fn select_edge(sql: &dyn Sql, item_id: &str, blocker_id: &str) -> Result<Option<Row>, SqlError> {
    Ok(sql
        .exec(
            "SELECT * FROM blocked_by WHERE item_id = ? AND blocker_id = ?",
            &[
                SqlValue::Text(item_id.to_string()),
                SqlValue::Text(blocker_id.to_string()),
            ],
        )?
        .into_iter()
        .next())
}

pub(super) fn edge_from_row(row: &Row) -> Result<BlockedBy, SqlError> {
    let r = RowReader(row);
    Ok(BlockedBy {
        item_id: r.text("item_id")?,
        blocker_id: r.text("blocker_id")?,
        version: r.int("version")?,
        removed_at: r.opt_int("removed_at"),
    })
}
