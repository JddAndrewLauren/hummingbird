//! Alerts: the pushed-context lane. `POST /api/alerts` (ingest scope) is a
//! version-blind upsert on `(source, source_key)` — webhook sources cannot
//! track versions, and the source is authoritative for its own fields
//! (ADR-0009 rule 3). `PATCH /api/alerts/:id` (device scope) writes the one
//! human-owned field, `dismissed_at`, under normal CAS.

use hummingbird_domain::{higher_severity, Alert, AlertIngest, AlertPatch};

use super::{
    auth, conflict, empty_status, error, json, parse_body, read_meta_version, write_meta_version,
    ApiResponse,
};
use crate::codec::{RowReader, Sets};
use crate::sql::{Row, Sql, SqlError, SqlValue};

/// `token_source` is the caller's bound source (#145) — `None` for a
/// legacy/raw-seeded ingest token, always `Some` for one minted through
/// `POST /api/admin/tokens`. A payload naming any other source is a 403
/// with an empty body, matching the rest of the scope-matrix error
/// semantics; it never reaches the upsert below.
pub fn ingest(
    body: Option<&str>,
    now_ms: i64,
    token_source: Option<&str>,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    let ingest: AlertIngest = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if ingest.source.is_empty() || ingest.source_key.is_empty() {
        return Ok(error(400, "validation", "source and source_key must be non-empty"));
    }
    if ingest.title.is_empty() {
        return Ok(error(400, "validation", "title must be non-empty"));
    }
    // Absent is always legal — most sources answer no standing question.
    // Present-but-empty is not: it is a `context_snapshots.key` that names
    // nothing, and the pane join would silently match no subject rather
    // than report the source's bug (ADR-0015's "visibly broken, never
    // quietly empty", applied on the alert side of the same join).
    if ingest.subject_key.as_deref() == Some("") {
        return Ok(error(400, "validation", "subject_key must be non-empty when present"));
    }
    // Two contradictory instructions about `raised_at`: one says "use this
    // stamp", the other says "decide the stamp for me". Refused rather than
    // silently ordered.
    if ingest.restamp_on_change && ingest.raised_at.is_some() {
        return Ok(error(
            400,
            "validation",
            "restamp_on_change and an explicit raised_at cannot both be sent",
        ));
    }
    if token_source != Some(ingest.source.as_str()) {
        return Ok(empty_status(403));
    }

    let (status, alert) = upsert(sql, now_ms, ingest)?;
    Ok(json(status, &alert))
}

/// The mint/ratchet core, HTTP-agnostic — shared verbatim by the webhook
/// ingest route above and #138's internal DO-alarm sweep, which mints
/// `item-threshold/v1` alerts through this exact same path rather than a
/// parallel one (the brief's "through the same path as every other kind").
/// Returns the HTTP status the caller would have answered with (201 first
/// raise, 200 no-op-or-ratcheted re-raise) alongside the resulting row —
/// the sweep ignores the status and uses only the alert.
pub(crate) fn upsert(
    sql: &dyn Sql,
    now_ms: i64,
    ingest: AlertIngest,
) -> Result<(u16, Alert), SqlError> {
    let Some(row) = select_by_identity(sql, &ingest.source, &ingest.source_key)? else {
        // First raise. The id is minted deterministically from the identity
        // — no entropy, and a crashed-and-replayed first raise lands on the
        // same row via the select above.
        let version = read_meta_version(sql)? + 1;
        let alert = Alert {
            id: deterministic_id(&ingest.source, &ingest.source_key),
            source: ingest.source,
            source_key: ingest.source_key,
            subject_key: ingest.subject_key,
            title: ingest.title,
            body: ingest.body,
            url: ingest.url,
            severity: ingest.severity,
            raised_at: ingest.raised_at.unwrap_or(now_ms),
            resolved_at: ingest.resolved_at,
            dismissed_at: None,
            expires_at: ingest.expires_at,
            version,
        };
        sql.exec(
            "INSERT INTO alerts (id, source, source_key, subject_key, title, body, url, severity, \
             raised_at, resolved_at, dismissed_at, expires_at, version) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?)",
            &[
                SqlValue::Text(alert.id.clone()),
                SqlValue::Text(alert.source.clone()),
                SqlValue::Text(alert.source_key.clone()),
                SqlValue::from_opt_text(alert.subject_key.as_deref()),
                SqlValue::Text(alert.title.clone()),
                SqlValue::from_opt_text(alert.body.as_deref()),
                SqlValue::from_opt_text(alert.url.as_deref()),
                SqlValue::from_opt_text(alert.severity.as_deref()),
                SqlValue::Integer(alert.raised_at),
                SqlValue::from_opt_i64(alert.resolved_at),
                SqlValue::from_opt_i64(alert.expires_at),
                SqlValue::Integer(alert.version),
            ],
        )?;
        write_meta_version(sql, version)?;
        return Ok((201, alert));
    };

    // Re-raise: the source-owned fields are set absolutely from the payload
    // (an absent optional clears — the source stopped sending it), with two
    // deliberate exceptions. An absent `raised_at` keeps the stored stamp,
    // so an identical replayed payload is a byte-identical state and the
    // upsert a no-op (AC1). `dismissed_at` is human-owned: never touched.
    //
    // `severity` is the other exception (ADR-0014): while the existing row
    // is still live, this re-raise is the same occurrence some other rule
    // or a later ping might also be minting against, so it may only raise
    // severity, never blind-overwrite it downward. Once the row has left
    // live — resolved, dismissed, or expired — the *next* raise starts a
    // fresh occurrence, and the payload's severity is authoritative as-is,
    // like every other source-owned field.
    let current = alert_from_row(&row)?;
    let severity = if current.is_live(now_ms) {
        higher_severity(current.severity.as_deref(), ingest.severity.as_deref()).map(str::to_string)
    } else {
        ingest.severity
    };
    let next = Alert {
        // Source-owned like title/body/url: set absolutely, so a source
        // that stops naming a subject clears the join rather than leaving
        // the alert bound to a subject it no longer claims.
        subject_key: ingest.subject_key,
        title: ingest.title,
        body: ingest.body,
        url: ingest.url,
        severity,
        raised_at: ingest.raised_at.unwrap_or(current.raised_at),
        resolved_at: ingest.resolved_at,
        expires_at: ingest.expires_at,
        ..current.clone()
    };
    if next == current {
        return Ok((200, current));
    }

    // Past this point the raise genuinely changed a source-owned field —
    // which is exactly the condition `restamp_on_change` names (#120). A
    // repeatedly-polled source re-reports the same occurrence every run for
    // days and cannot tell those runs apart from a correction, because an
    // ingest token cannot read the alert back; the equality check above is
    // the server making that call on its behalf. `now_ms` is the **write
    // clock**, deliberately, not the poller's nominal cron slot — a
    // correction stamped at an 06:00 bucket would land before an 08:00
    // dismissal made the same morning and stay silently quiet.
    //
    // Note where this sits: the unchanged re-poll returned above without a
    // write at all, so the dismissal it must not disturb is never even
    // rewritten. `dismissed_at` itself stays untouched here as everywhere —
    // what a restamp does is let a later `raised_at` overtake it, which is
    // ADR-0014's own "a later raise rings again over a settled alert".
    let next = if ingest.restamp_on_change {
        Alert { raised_at: now_ms, ..next }
    } else {
        next
    };

    let version = read_meta_version(sql)? + 1;
    sql.exec(
        "UPDATE alerts SET subject_key = ?, title = ?, body = ?, url = ?, severity = ?, \
         raised_at = ?, resolved_at = ?, expires_at = ?, version = ? WHERE id = ?",
        &[
            SqlValue::from_opt_text(next.subject_key.as_deref()),
            SqlValue::Text(next.title.clone()),
            SqlValue::from_opt_text(next.body.as_deref()),
            SqlValue::from_opt_text(next.url.as_deref()),
            SqlValue::from_opt_text(next.severity.as_deref()),
            SqlValue::Integer(next.raised_at),
            SqlValue::from_opt_i64(next.resolved_at),
            SqlValue::from_opt_i64(next.expires_at),
            SqlValue::Integer(version),
            SqlValue::Text(next.id.clone()),
        ],
    )?;
    write_meta_version(sql, version)?;
    Ok((200, Alert { version, ..next }))
}

/// The device lane: dismiss (or un-dismiss) under CAS — the only alert
/// field a device may write.
pub fn dismiss(
    id: &str,
    body: Option<&str>,
    _now_ms: i64,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    let patch: AlertPatch = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };

    let Some(row) = select_by_id(sql, id)? else {
        return Ok(error(404, "not_found", "no such alert"));
    };
    let current = alert_from_row(&row)?;
    if current.version != patch.expected_version {
        return Ok(conflict(&current));
    }

    // Compared typed against `current`, never a bare SQL-value `==` — see
    // items::patch for why.
    let mut sets = Sets::new();
    if let Some(dismissed_at) = patch.dismissed_at {
        if dismissed_at != current.dismissed_at {
            sets.set("dismissed_at", SqlValue::from_opt_i64(dismissed_at));
        }
    }
    if sets.is_empty() {
        return Ok(json(200, &current));
    }

    let version = read_meta_version(sql)? + 1;
    sets.set("version", SqlValue::Integer(version));
    let update = sets.update_sql("alerts", "id = ?");
    let mut params = sets.into_params();
    params.push(SqlValue::Text(id.to_string()));
    sql.exec(&update, &params)?;
    write_meta_version(sql, version)?;

    let row = select_by_id(sql, id)?.ok_or_else(|| SqlError {
        message: "row vanished mid-update".into(),
    })?;
    Ok(json(200, &alert_from_row(&row)?))
}

/// `hex(sha256("alert:" + len(source) + ":" + source + ":" + source_key))[..32]`
/// — stable across raises by construction. The length prefix keeps the
/// preimage unambiguous: without it, `("app", "db:prod")` and
/// `("app:db", "prod")` would collide on id while differing on the real
/// identity columns, and the second raise would hit the primary key forever.
fn deterministic_id(source: &str, source_key: &str) -> String {
    let mut id = auth::sha256_hex(&format!("alert:{}:{source}:{source_key}", source.len()));
    id.truncate(32);
    id
}

/// The mint/ratchet core's own identity read, exposed so a caller can
/// decide something *before* calling [`upsert`] — #138's sweep uses this to
/// tell "still live, keep the stamp" apart from "settled or absent, a fresh
/// occurrence starts now" ahead of picking what `raised_at` to pass in.
pub(crate) fn find_by_identity(
    sql: &dyn Sql,
    source: &str,
    source_key: &str,
) -> Result<Option<Alert>, SqlError> {
    select_by_identity(sql, source, source_key)?.map(|row| alert_from_row(&row)).transpose()
}

/// Every currently-live alert of one source, for ADR-0014's resolution pass
/// (#217) — which iterates *alerts*, not items, because an item-side scan
/// can only reach the alerts whose items it still sees, precisely the set
/// that does not need resolving.
///
/// The three-clause live predicate is applied in Rust, through
/// [`Alert::is_live`], rather than transcribed into this `WHERE`. It is
/// subtle (each lifecycle stamp holds only until a later raise overtakes
/// it), it is ADR-0014's normative definition, and a second copy in SQL is
/// a place for the two to drift silently. The scan is bounded by one
/// source's alert rows, which the operator keeps small by construction.
pub(crate) fn find_live_by_source(
    sql: &dyn Sql,
    source: &str,
    now_ms: i64,
) -> Result<Vec<Alert>, SqlError> {
    let rows = sql.exec(
        "SELECT * FROM alerts WHERE source = ?",
        &[SqlValue::Text(source.to_string())],
    )?;
    let mut live = Vec::new();
    for row in &rows {
        let alert = alert_from_row(row)?;
        if alert.is_live(now_ms) {
            live.push(alert);
        }
    }
    Ok(live)
}

/// Stamps `resolved_at` on one alert — ADR-0014's "the condition ended",
/// said by the only party that can know it.
///
/// Deliberately *not* routed through [`upsert`]: that is the **source's**
/// path, which sets every source-owned field absolutely from a payload. The
/// resolution pass has no payload — it is not re-reporting the alert, it is
/// closing it — so it writes the single column it owns and leaves title,
/// body, url, severity and `raised_at` exactly as the last raise left them.
/// Resolution is not dismissal: `dismissed_at` stays untouched (human-owned),
/// and a later raise overtakes this stamp and rings again, which is what
/// keying `item-threshold/v1` on `item:<id>` bought.
///
/// Version-bumped through the same `meta` counter as every other write, so
/// a resolved alert reaches devices on the next delta pull rather than
/// waiting for the daily `GET /api/sweep`.
pub(crate) fn resolve(sql: &dyn Sql, alert: &Alert, now_ms: i64) -> Result<Alert, SqlError> {
    let version = read_meta_version(sql)? + 1;
    sql.exec(
        "UPDATE alerts SET resolved_at = ?, version = ? WHERE id = ?",
        &[
            SqlValue::Integer(now_ms),
            SqlValue::Integer(version),
            SqlValue::Text(alert.id.clone()),
        ],
    )?;
    write_meta_version(sql, version)?;
    Ok(Alert { resolved_at: Some(now_ms), version, ..alert.clone() })
}

fn select_by_identity(sql: &dyn Sql, source: &str, source_key: &str) -> Result<Option<Row>, SqlError> {
    Ok(sql
        .exec(
            "SELECT * FROM alerts WHERE source = ? AND source_key = ?",
            &[
                SqlValue::Text(source.to_string()),
                SqlValue::Text(source_key.to_string()),
            ],
        )?
        .into_iter()
        .next())
}

fn select_by_id(sql: &dyn Sql, id: &str) -> Result<Option<Row>, SqlError> {
    Ok(sql
        .exec(
            "SELECT * FROM alerts WHERE id = ?",
            &[SqlValue::Text(id.to_string())],
        )?
        .into_iter()
        .next())
}

pub(super) fn alert_from_row(row: &Row) -> Result<Alert, SqlError> {
    let r = RowReader(row);
    Ok(Alert {
        id: r.text("id")?,
        source: r.text("source")?,
        source_key: r.text("source_key")?,
        subject_key: r.opt_text("subject_key"),
        title: r.text("title")?,
        body: r.opt_text("body"),
        url: r.opt_text("url"),
        severity: r.opt_text("severity"),
        raised_at: r.int("raised_at")?,
        resolved_at: r.opt_int("resolved_at"),
        dismissed_at: r.opt_int("dismissed_at"),
        expires_at: r.opt_int("expires_at"),
        version: r.int("version")?,
    })
}
