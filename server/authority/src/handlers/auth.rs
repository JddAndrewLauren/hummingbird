//! Bearer-token authentication and the scope matrix (ADR-0008). 401 means
//! the credential itself is bad — missing, unknown, or revoked, deliberately
//! indistinguishable — and is what tells a client to re-prompt (ADR-0004 as
//! ported). 403 means the token is fine and the operation is out of scope;
//! it must never trigger a re-prompt. Both bodies are empty: clean status,
//! no leakage.

use hummingbird_domain::Scope;
use sha2::{Digest, Sha256};

use crate::codec::{bad_cell, RowReader};
use crate::sql::{Sql, SqlError, SqlValue};

pub fn sha256_hex(input: &str) -> String {
    hex(&Sha256::digest(input.as_bytes()))
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
        write!(out, "{b:02x}").expect("writing to a String cannot fail");
        out
    })
}

fn parse_bearer(header: Option<&str>) -> Option<&str> {
    let (scheme, token) = header?.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then_some(token)
}

/// Admin gate: comparing digests doubles as constant-time equality, and an
/// unconfigured `ADMIN_SECRET` fails closed.
pub fn admin_ok(authorization: Option<&str>, admin_secret: Option<&str>) -> bool {
    match (parse_bearer(authorization), admin_secret) {
        (Some(provided), Some(secret)) => sha256_hex(provided) == sha256_hex(secret),
        _ => false,
    }
}

/// A resolved bearer token: its scope, plus the source it is bound to when
/// `scope` is `ingest` (#145) — `None` for `device`/`sweeper`, always
/// `Some` for a properly minted `ingest` token (a null-source `ingest` row
/// can only exist via a raw seam that bypassed the mint handler).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub scope: Scope,
    pub source: Option<String>,
}

/// Resolve the bearer to a live token's principal, stamping `last_seen` on
/// the way — with **no meta bump**: tokens sit outside the delta contract,
/// and bumping here would make every authed read dirty the cursor.
pub fn authenticate(
    authorization: Option<&str>,
    now_ms: i64,
    sql: &dyn Sql,
) -> Result<Option<Principal>, SqlError> {
    let Some(token) = parse_bearer(authorization) else {
        return Ok(None);
    };
    let Some(row) = sql
        .exec(
            "SELECT * FROM tokens WHERE token_hash = ? AND revoked_at IS NULL",
            &[SqlValue::Text(sha256_hex(token))],
        )?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    let r = RowReader(&row);
    let scope_text = r.text("scope")?;
    let scope = Scope::parse(&scope_text).ok_or_else(|| bad_cell("scope"))?;
    let source = r.opt_text("source");
    let id = r.text("id")?;
    sql.exec(
        "UPDATE tokens SET last_seen = ? WHERE id = ?",
        &[SqlValue::Integer(now_ms), SqlValue::Text(id)],
    )?;
    Ok(Some(Principal { scope, source }))
}

/// The whole scope matrix. Device is a full task client: everything except
/// alert ingest. Sweeper is its retarget contract and nothing else:
/// `POST /api/items`. Ingest is the webhook lane and nothing else:
/// `POST /api/alerts`. Admin routes never reach here.
///
/// This is the scope check only — it has no access to the request body, so
/// it cannot see which source a `POST /api/alerts` payload names. The
/// source-binding check (#145: an ingest token may only post for its own
/// source) lives in the alerts handler, which does parse the body.
pub fn permitted(scope: Scope, method: &str, segments: &[&str]) -> bool {
    match (method, segments) {
        ("POST", ["items"]) => matches!(scope, Scope::Device | Scope::Sweeper),
        ("POST", ["alerts"]) => matches!(scope, Scope::Ingest),
        ("POST", ["snapshots"]) => matches!(scope, Scope::Ingest),
        // The first read a non-device scope got (#120; #135-137 below add
        // two more, `rules` and `snapshots`). An ingest token's
        // poller needs the binding that tells it *what* to poll, and the
        // alternative — duplicating that URL into the poller's own
        // environment — would give the one fact the binding editor exists to
        // own two sources of truth and make the editor decorative.
        //
        // The widening is real and worth stating plainly: an ingest token
        // can read ANY setting by name, not just one belonging to its own
        // source. What bounds it is that `settings` is a small closed
        // vocabulary of binding facts (`client/core/src/bindings.rs`), it
        // holds no credential, and the route is read-only — a token that
        // could already write alerts and snapshots for its source gains no
        // ability to change anything here.
        ("GET", ["settings", _]) => matches!(scope, Scope::Device | Scope::Ingest),
        // The evaluated-stream pollers' own reads (#135-137). An
        // out-of-process poller (Gmail, Google Calendar, M365 mail/calendar)
        // evaluates rules in memory against its own fetched batch
        // (ADR-0011) and needs the operator's live rule set to do it, and
        // its own previously-written cursor so a restart resumes rather
        // than replays (ADR-0011's "per-source delta cursor"). Neither
        // widens what an ingest token can already reach in kind — it could
        // already write `alerts`/`snapshots` for its bound source; reading
        // its own snapshot rows (source-bound below, in `snapshots::get`)
        // does not let it write anything new. **`rules` is not the same
        // shape as `settings`**, unlike the carve-out above — it is
        // operator-authored content, not a small closed vocabulary of
        // binding facts — so this scope check alone would hand every
        // ingest token (including `CITY_WASTE_INGEST_TOKEN`, which sits in
        // GitHub Actions) the operator's entire rule set. `rules::list`
        // itself narrows the *response*, not this gate, to only the rules
        // the calling token's bound source is entitled to see
        // (`rules::event_kinds_readable_by`) — the same shape as the
        // source-binding check on `snapshots::get` and the alert/snapshot
        // write lanes, just applied inside the handler because it depends
        // on each rule's own `event_kind`, not just the request.
        ("GET", ["rules"]) => matches!(scope, Scope::Device | Scope::Ingest),
        ("GET", ["snapshots"]) => matches!(scope, Scope::Device | Scope::Ingest),
        _ => matches!(scope, Scope::Device),
    }
}
