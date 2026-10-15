//! `GET /api/devices` and `DELETE /api/devices/:id` — a `device` token sees
//! and removes the operator's devices (ADR-0040 decision 6). The admin lane
//! (`admin_tokens.rs`) is unchanged and still sees every scope; this lane is
//! the `device`-scope slice of the same table, and nothing else:
//!
//! - **Only `device` rows exist here.** A `sweeper` or `ingest` token is never
//!   listed, and a DELETE naming one answers 404 exactly as an unknown id
//!   does — a device cannot learn which non-device ids exist.
//! - **Two device holders are listed but refused**, from the reviewed
//!   [`NON_DEVICE_HOLDERS`]: 403, empty, like every other in-scope refusal.
//! - **Revoking is the admin lane's soft revoke** (`revoked_at`, idempotent).
//!   Revoking the caller's own id is signing out: this response is a 200,
//!   and the next request it makes is a 401 from `auth::authenticate`.
//!
//! Like every token write, neither route bumps the workspace counter:
//! tokens are machinery outside the delta contract.

use hummingbird_domain::{DeviceInfo, DeviceListPlatform, Scope};

use super::admin_tokens::select_token;
use super::{empty_status, error, json, ApiResponse};
use crate::codec::RowReader;
use crate::sql::{Row, Sql, SqlError, SqlValue};

/// The `device`-scope token ids a device may not revoke (ADR-0040
/// decision 6): removing either needs a terminal re-mint and a secret
/// rotation elsewhere (ADR-0018 for the runner, `docs/openclaw.md` for the
/// agent), and a tap in Settings should never start that. Modelled on
/// `calendar_token::CALENDAR_WRITE_HOLDERS`: a reviewed `const` with a
/// membership test, so a change is a one-line diff plus a test. The admin
/// lane can still revoke both.
pub const NON_DEVICE_HOLDERS: &[&str] = &["runner", "openclaw-agent"];

pub fn list(caller_id: &str, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let devices = sql
        .exec(
            "SELECT * FROM tokens WHERE scope = ? ORDER BY created_at, id",
            &[SqlValue::Text(Scope::Device.as_str().to_string())],
        )?
        .iter()
        .map(|row| device_from_row(row, caller_id))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json(200, &devices))
}

pub fn revoke(
    id: &str,
    caller_id: &str,
    now_ms: i64,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    if NON_DEVICE_HOLDERS.contains(&id) {
        return Ok(empty_status(403));
    }
    let Some(row) = select_token(sql, id)?.filter(is_device) else {
        return Ok(error(404, "not_found", "no such device"));
    };
    if RowReader(&row).opt_int("revoked_at").is_none() {
        sql.exec(
            "UPDATE tokens SET revoked_at = ? WHERE id = ?",
            &[SqlValue::Integer(now_ms), SqlValue::Text(id.to_string())],
        )?;
    }
    let row = select_token(sql, id)?.ok_or_else(|| SqlError {
        message: "row vanished mid-update".into(),
    })?;
    Ok(json(200, &device_from_row(&row, caller_id)?))
}

fn is_device(row: &Row) -> bool {
    RowReader(row).opt_text("scope").as_deref() == Some(Scope::Device.as_str())
}

/// Everything a device may see of a `device` row — never the hash.
fn device_from_row(row: &Row, caller_id: &str) -> Result<DeviceInfo, SqlError> {
    let r = RowReader(row);
    let id = r.text("id")?;
    Ok(DeviceInfo {
        platform: DeviceListPlatform::of_token_id(&id),
        is_current: id == caller_id,
        revocable: !NON_DEVICE_HOLDERS.contains(&id.as_str()),
        name: r.text("name")?,
        created_at: r.int("created_at")?,
        last_seen: r.opt_int("last_seen"),
        revoked_at: r.opt_int("revoked_at"),
        id,
    })
}

