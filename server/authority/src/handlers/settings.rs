//! `PUT /api/settings/:key` — workspace preferences. The key is the
//! identity, so PUT carries both create (`expected_version: 0`) and update
//! semantics under one CAS rule; `value` arrives as typed JSON and is
//! stored as its canonical serialization.

use hummingbird_domain::{PutSetting, Setting};

use super::{conflict, error, json, parse_body, read_meta_version, write_meta_version, ApiResponse};
use crate::codec::RowReader;
use crate::sql::{Row, Sql, SqlError, SqlValue};

pub fn put(
    key: &str,
    body: Option<&str>,
    now_ms: i64,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    let put: PutSetting = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    let value = put.value.to_string();

    let current = match select_setting(sql, key)? {
        Some(row) => Some(setting_from_row(&row)?),
        None => None,
    };
    match current {
        None if put.expected_version == 0 => {
            let version = read_meta_version(sql)? + 1;
            let setting = Setting {
                key: key.to_string(),
                value,
                updated_at: now_ms,
                version,
            };
            sql.exec(
                "INSERT INTO settings (key, value, updated_at, version) VALUES (?, ?, ?, ?)",
                &[
                    SqlValue::Text(setting.key.clone()),
                    SqlValue::Text(setting.value.clone()),
                    SqlValue::Integer(setting.updated_at),
                    SqlValue::Integer(setting.version),
                ],
            )?;
            write_meta_version(sql, version)?;
            Ok(json(201, &setting))
        }
        None => Ok(error(404, "not_found", "no such setting")),
        // A create replay (expected 0 on an existing key) is answered with
        // the stored row, divergent value or not — the items rule.
        Some(current) if put.expected_version == 0 => Ok(json(200, &current)),
        Some(current) if current.version != put.expected_version => Ok(conflict(&current)),
        // Identical value: the empty-patch rule — no write, no bump.
        Some(current) if current.value == value => Ok(json(200, &current)),
        Some(_) => {
            let version = read_meta_version(sql)? + 1;
            sql.exec(
                "UPDATE settings SET value = ?, updated_at = ?, version = ? WHERE key = ?",
                &[
                    SqlValue::Text(value),
                    SqlValue::Integer(now_ms),
                    SqlValue::Integer(version),
                    SqlValue::Text(key.to_string()),
                ],
            )?;
            write_meta_version(sql, version)?;
            let row = select_setting(sql, key)?.ok_or_else(|| SqlError {
                message: "row vanished mid-update".into(),
            })?;
            Ok(json(200, &setting_from_row(&row)?))
        }
    }
}

/// `GET /api/settings/:key` — one row by key, 404 if unset.
///
/// The one read a non-device scope reaches (see `auth::permitted`). It
/// exists because a poller needs the binding that tells it *what* to poll,
/// and there is no other way to ask: `GET /api/changes` is device-only, and
/// duplicating the value into the poller's own environment would give the
/// one fact the binding editor exists to own two sources of truth and make
/// that editor decorative.
///
/// 404 rather than a 200 with a null: "nobody has set this" is a state the
/// caller must handle (`client/core/src/bindings.rs`'s `Unset` makes the
/// same distinction), and a poller's correct response to it is to exit
/// without writing anything rather than to poll a guessed address.
pub fn get(key: &str, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    match select_setting(sql, key)? {
        Some(row) => Ok(json(200, &setting_from_row(&row)?)),
        None => Ok(error(404, "not_found", "no such setting")),
    }
}

fn select_setting(sql: &dyn Sql, key: &str) -> Result<Option<Row>, SqlError> {
    Ok(sql
        .exec(
            "SELECT * FROM settings WHERE key = ?",
            &[SqlValue::Text(key.to_string())],
        )?
        .into_iter()
        .next())
}

pub(super) fn setting_from_row(row: &Row) -> Result<Setting, SqlError> {
    let r = RowReader(row);
    Ok(Setting {
        key: r.text("key")?,
        value: r.text("value")?,
        updated_at: r.int("updated_at")?,
        version: r.int("version")?,
    })
}
