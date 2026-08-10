//! `POST /api/push_targets` and `DELETE /api/push_targets/:id` (#139): FCM
//! registration and individual revocation — the notification lane's
//! sibling to `tokens` (ADR-0012), minted by #131's schema with its HTTP
//! surface deferred here. Create is idempotent by client-supplied id, like
//! `rules::create`, with one deliberate departure: unlike an item or rule
//! id, a `push_target` id names a *device slot* whose `fcm_token` FCM
//! rotates over the device's lifetime with no new id minted for it, so a
//! replay that names a changed `name`/`platform`/`fcm_token` adopts the new
//! values rather than silently keeping the stale ones — and, since a fresh
//! registration is the device announcing it is live again, it also revives
//! a target that had been revoked (the operator can always re-revoke it).
//! `revoke` is a flag, never a delete, and idempotent like
//! `admin_tokens::revoke`.

use hummingbird_domain::{CreatePushTarget, Platform, PushTarget};

use super::{error, json, parse_body, ApiResponse};
use crate::codec::{bad_cell, RowReader, Sets};
use crate::sql::{Row, Sql, SqlError, SqlValue};

pub fn register(body: Option<&str>, now_ms: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let create: CreatePushTarget = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if create.id.is_empty() {
        return Ok(error(400, "validation", "id must be non-empty"));
    }

    if let Some(row) = select(sql, &create.id)? {
        let current = push_target_from_row(&row)?;
        let unchanged = current.name == create.name
            && current.platform == create.platform
            && current.fcm_token == create.fcm_token
            && current.revoked_at.is_none();
        if unchanged {
            return Ok(json(200, &current));
        }
        if create.name.is_empty() {
            return Ok(error(400, "validation", "name must be non-empty"));
        }
        if create.fcm_token.is_empty() {
            return Ok(error(400, "validation", "fcm_token must be non-empty"));
        }
        // A changed token/name/platform, or a fresh registration on a
        // revoked id, is the device re-announcing itself live: adopt the
        // new values and clear any revocation.
        let mut sets = Sets::new();
        sets.set("name", SqlValue::Text(create.name.clone()));
        sets.set("platform", SqlValue::Text(create.platform.as_str().to_string()));
        sets.set("fcm_token", SqlValue::Text(create.fcm_token.clone()));
        sets.set("revoked_at", SqlValue::Null);
        let update = sets.update_sql("push_targets", "id = ?");
        let mut params = sets.into_params();
        params.push(SqlValue::Text(create.id.clone()));
        sql.exec(&update, &params)?;
        let row = select(sql, &create.id)?.ok_or_else(|| SqlError {
            message: "row vanished mid-update".into(),
        })?;
        return Ok(json(200, &push_target_from_row(&row)?));
    }

    if create.name.is_empty() {
        return Ok(error(400, "validation", "name must be non-empty"));
    }
    if create.fcm_token.is_empty() {
        return Ok(error(400, "validation", "fcm_token must be non-empty"));
    }

    let target = PushTarget {
        id: create.id,
        name: create.name,
        platform: create.platform,
        fcm_token: create.fcm_token,
        created_at: now_ms,
        last_seen: None,
        revoked_at: None,
    };
    sql.exec(
        "INSERT INTO push_targets (id, name, platform, fcm_token, created_at) \
         VALUES (?, ?, ?, ?, ?)",
        &[
            SqlValue::Text(target.id.clone()),
            SqlValue::Text(target.name.clone()),
            SqlValue::Text(target.platform.as_str().to_string()),
            SqlValue::Text(target.fcm_token.clone()),
            SqlValue::Integer(target.created_at),
        ],
    )?;
    Ok(json(201, &target))
}

/// Revocation is a flag, never a delete — a revoked target's history (and
/// any delivery that targeted it) stays intact, and a lost or reset device
/// can be dropped without disturbing its siblings. Idempotent: revoking an
/// already-revoked target returns it unchanged.
pub fn revoke(id: &str, now_ms: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let Some(row) = select(sql, id)? else {
        return Ok(error(404, "not_found", "no such push target"));
    };
    let current = push_target_from_row(&row)?;
    if current.revoked_at.is_some() {
        return Ok(json(200, &current));
    }
    sql.exec(
        "UPDATE push_targets SET revoked_at = ? WHERE id = ?",
        &[SqlValue::Integer(now_ms), SqlValue::Text(id.to_string())],
    )?;
    let row = select(sql, id)?.ok_or_else(|| SqlError {
        message: "row vanished mid-update".into(),
    })?;
    Ok(json(200, &push_target_from_row(&row)?))
}

fn select(sql: &dyn Sql, id: &str) -> Result<Option<Row>, SqlError> {
    Ok(sql
        .exec(
            "SELECT * FROM push_targets WHERE id = ?",
            &[SqlValue::Text(id.to_string())],
        )?
        .into_iter()
        .next())
}

/// `pub(crate)`: also read by [`crate::delivery`], which sends to every
/// live (non-revoked) target — the row shape is owned here, once.
pub(crate) fn push_target_from_row(row: &Row) -> Result<PushTarget, SqlError> {
    let r = RowReader(row);
    let platform_text = r.text("platform")?;
    Ok(PushTarget {
        id: r.text("id")?,
        name: r.text("name")?,
        platform: Platform::parse(&platform_text).ok_or_else(|| bad_cell("platform"))?,
        fcm_token: r.text("fcm_token")?,
        created_at: r.int("created_at")?,
        last_seen: r.opt_int("last_seen"),
        revoked_at: r.opt_int("revoked_at"),
    })
}
