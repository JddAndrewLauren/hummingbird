//! `POST|GET /api/admin/tokens` and `DELETE /api/admin/tokens/:id` — mint,
//! list, revoke. Gated by `ADMIN_SECRET` upstream (see `auth.rs`); used
//! only from the operator's terminal. Tokens never bump the workspace
//! counter: they are machinery outside the delta contract.

use hummingbird_domain::{find_source, MintToken, MintedToken, Scope, TokenInfo};

use super::{auth, error, json, parse_body, ApiResponse, HandleContext};
use crate::codec::{bad_cell, RowReader};
use crate::sql::{Row, Sql, SqlError, SqlValue};

pub fn mint(
    body: Option<&str>,
    ctx: &HandleContext,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    let mint: MintToken = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if mint.id.is_empty() {
        return Ok(error(400, "validation", "id must be non-empty"));
    }

    // Idempotent by client-supplied id — but the plaintext exists only in
    // the original 201 (only its hash is stored), so a replay returns the
    // metadata without it. The select runs before the remaining validation:
    // already-exists is success, even under a divergent payload.
    if let Some(row) = select_token(sql, &mint.id)? {
        return Ok(json(200, &token_info_from_row(&row)?));
    }

    if mint.name.is_empty() {
        return Ok(error(400, "validation", "name must be non-empty"));
    }
    // ADR-0008's "one token per ingest source": the pairing is required one
    // way and forbidden the other (#145).
    if mint.scope == Scope::Ingest && mint.source.is_none() {
        return Ok(error(400, "validation", "an ingest token requires a source"));
    }
    if mint.scope != Scope::Ingest && mint.source.is_some() {
        return Ok(error(
            400,
            "validation",
            "only an ingest token may carry a source",
        ));
    }
    // ADR-0014's registry is the only source of truth for what an alert
    // source is (#189): an ingest token bound to a string the registry
    // has never heard of would 403 on every alert it ever posts, silently
    // and indistinguishably from a genuinely wrong-source push; one bound
    // to a **retired** source is the same drift the registry exists to
    // make loud ("nothing new should be minted under it").
    if let Some(source) = &mint.source {
        match find_source(source) {
            None => {
                return Ok(error(
                    400,
                    "validation",
                    &format!("`{source}` is not a registered alert source"),
                ));
            }
            Some(entry) => {
                if let Some(successor) = entry.retired_as {
                    return Ok(error(
                        400,
                        "validation",
                        &format!(
                            "`{source}` is retired (bumped to `{successor}`); nothing new may be minted under it"
                        ),
                    ));
                }
            }
        }
    }

    let mut bytes = [0u8; 32];
    ctx.entropy.fill(&mut bytes);
    let token = format!("hb_{}", auth::hex(&bytes));
    sql.exec(
        "INSERT INTO tokens (id, name, scope, source, token_hash, created_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
        &[
            SqlValue::Text(mint.id.clone()),
            SqlValue::Text(mint.name.clone()),
            SqlValue::Text(mint.scope.as_str().to_string()),
            SqlValue::from_opt_text(mint.source.as_deref()),
            SqlValue::Text(auth::sha256_hex(&token)),
            SqlValue::Integer(ctx.now_ms),
        ],
    )?;
    Ok(json(
        201,
        &MintedToken {
            id: mint.id,
            name: mint.name,
            scope: mint.scope,
            source: mint.source,
            created_at: ctx.now_ms,
            token,
        },
    ))
}

pub fn list(sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let tokens = sql
        .exec("SELECT * FROM tokens ORDER BY created_at, id", &[])?
        .iter()
        .map(token_info_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json(200, &tokens))
}

/// Revocation is a flag (`revoked_at`), and idempotent: revoking an
/// already-revoked token returns it unchanged.
pub fn revoke(id: &str, now_ms: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let Some(row) = select_token(sql, id)? else {
        return Ok(error(404, "not_found", "no such token"));
    };
    let current = token_info_from_row(&row)?;
    if current.revoked_at.is_some() {
        return Ok(json(200, &current));
    }
    sql.exec(
        "UPDATE tokens SET revoked_at = ? WHERE id = ?",
        &[SqlValue::Integer(now_ms), SqlValue::Text(id.to_string())],
    )?;
    let row = select_token(sql, id)?.ok_or_else(|| SqlError {
        message: "row vanished mid-update".into(),
    })?;
    Ok(json(200, &token_info_from_row(&row)?))
}

fn select_token(sql: &dyn Sql, id: &str) -> Result<Option<Row>, SqlError> {
    Ok(sql
        .exec(
            "SELECT * FROM tokens WHERE id = ?",
            &[SqlValue::Text(id.to_string())],
        )?
        .into_iter()
        .next())
}

/// Everything in the row except the hash — the one column that never
/// leaves the server.
fn token_info_from_row(row: &Row) -> Result<TokenInfo, SqlError> {
    let r = RowReader(row);
    let scope_text = r.text("scope")?;
    Ok(TokenInfo {
        id: r.text("id")?,
        name: r.text("name")?,
        scope: hummingbird_domain::Scope::parse(&scope_text).ok_or_else(|| bad_cell("scope"))?,
        source: r.opt_text("source"),
        created_at: r.int("created_at")?,
        last_seen: r.opt_int("last_seen"),
        revoked_at: r.opt_int("revoked_at"),
    })
}
