//! `POST /api/snapshots` — the server-polled context lane's write side
//! (ADR-0009's gauges, ADR-0015's envelope). Modelled on
//! [`super::alerts::ingest`]: `ingest` scope, version-blind, identity in the
//! body, and the source authoritative for its own row.
//!
//! **Generic, not waste-specific.** ADR-0009 names three more callers for
//! this lane (usage, GitHub stats, races) and they differ only in what they
//! put in `payload`. Identity is in the body rather than the path for a
//! mechanical reason as well as a taste one: `handlers/mod.rs` splits the
//! path on `/` with no percent-decoding, and every source string contains a
//! slash by construction (`city-waste/v2`), so `POST /api/snapshots/:source/:key`
//! could not be routed without either encoding the source or growing a
//! grammar over path segments.

use hummingbird_domain::{find_source, ContextSnapshot, SnapshotEnvelope, SnapshotIngest};

use super::{
    empty_status, error, json, parse_body, query_param, read_meta_version, write_meta_version,
    ApiResponse,
};
use crate::codec::RowReader;
use crate::sql::{Row, Sql, SqlError, SqlValue};

/// `token_source` is the caller's bound source (#145), exactly as on the
/// alert lane: a payload naming any other source is an empty-bodied 403 and
/// never reaches the upsert.
pub fn ingest(
    body: Option<&str>,
    _now_ms: i64,
    token_source: Option<&str>,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    let ingest: SnapshotIngest = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if ingest.source.is_empty() || ingest.key.is_empty() {
        return Ok(error(400, "validation", "source and key must be non-empty"));
    }
    // A snapshot with no read time cannot be aged, and a pane that cannot
    // age an answer must not render it as fresh (`Freshness::Unknown`'s own
    // rule). A non-positive stamp is a poller bug, not a legitimate epoch.
    if ingest.fetched_at <= 0 {
        return Ok(error(400, "validation", "fetched_at must be a positive epoch-ms stamp"));
    }

    let payload = ingest.payload.to_string();
    // ADR-0015's envelope is the one thing checked about the payload, and
    // it is checked *here* rather than left to the reader: a stored row
    // that no pane can parse is a source silently answering nothing, and
    // the poller that wrote it gets a 400 it can log instead.
    //
    // Two checks deliberately absent. `schema` is NOT looked up in
    // `REGISTRY` — ADR-0015 forbids it, since a schema this build has not
    // heard of is a fact about the build. And `schema` need not equal
    // `source`: schema names the payload shape, source names the feed. They
    // coincide for `city-waste/v2`; nothing couples them.
    if let Err(problem) = SnapshotEnvelope::parse(&payload) {
        return Ok(error(400, "validation", &problem.to_string()));
    }
    // ADR-0014's 2026-08-11 amendment (#254): the mirror of the check in
    // `alerts.rs::ingest` — a source the registry knows and does not
    // declare for `context_snapshots` (e.g. `item-threshold/v1`, alerts-only)
    // is rejected here rather than silently writing a table it was never
    // enrolled to write. An unenrolled source is left alone; enrollment is
    // the mint gate's job.
    if let Some(entry) = find_source(&ingest.source) {
        if !entry.writes_snapshots() {
            return Ok(error(
                400,
                "validation",
                &format!("`{}` is not declared for snapshots", ingest.source),
            ));
        }
    }
    if token_source != Some(ingest.source.as_str()) {
        return Ok(empty_status(403));
    }

    let current = match select_snapshot(sql, &ingest.source, &ingest.key)? {
        Some(row) => Some(snapshot_from_row(&row)?),
        None => None,
    };

    let Some(current) = current else {
        let version = read_meta_version(sql)? + 1;
        let snapshot = ContextSnapshot {
            source: ingest.source,
            key: ingest.key,
            payload,
            fetched_at: ingest.fetched_at,
            version,
        };
        sql.exec(
            "INSERT INTO context_snapshots (source, key, payload, fetched_at, version) \
             VALUES (?, ?, ?, ?, ?)",
            &[
                SqlValue::Text(snapshot.source.clone()),
                SqlValue::Text(snapshot.key.clone()),
                SqlValue::Text(snapshot.payload.clone()),
                SqlValue::Integer(snapshot.fetched_at),
                SqlValue::Integer(snapshot.version),
            ],
        )?;
        write_meta_version(sql, version)?;
        return Ok(json(201, &snapshot));
    };

    // A poll that arrived out of order — a retried run finishing after the
    // next one, two runners overlapping — must never walk the stored answer
    // backwards. Answered 200 with the stored row, not a 409: the poller has
    // nothing to rebase and nothing to do about it.
    if ingest.fetched_at < current.fetched_at {
        return Ok(json(200, &current));
    }

    // The no-write rule, and its one deliberate narrowness: **exact replay
    // only**. `fetched_at` is part of the value, so a fresh poll that read
    // the same answer still writes and still bumps. Skipping that write
    // would freeze the stamp `Freshness::of_snapshot` and each pane's own
    // staleness threshold read from, and the pane would go stale on day two
    // of a perfectly healthy week — turning "poller fine, nothing changed"
    // into "poller dead", which is the exact discrimination this lane
    // exists to make. What is skipped is the byte-identical re-POST (a
    // retry, a double-fired cron), which carries no new reading at all.
    if current.payload == payload && current.fetched_at == ingest.fetched_at {
        return Ok(json(200, &current));
    }

    let version = read_meta_version(sql)? + 1;
    sql.exec(
        "UPDATE context_snapshots SET payload = ?, fetched_at = ?, version = ? \
         WHERE source = ? AND key = ?",
        &[
            SqlValue::Text(payload.clone()),
            SqlValue::Integer(ingest.fetched_at),
            SqlValue::Integer(version),
            SqlValue::Text(ingest.source.clone()),
            SqlValue::Text(ingest.key.clone()),
        ],
    )?;
    write_meta_version(sql, version)?;
    Ok(json(
        200,
        &ContextSnapshot {
            source: ingest.source,
            key: ingest.key,
            payload,
            fetched_at: ingest.fetched_at,
            version,
        },
    ))
}

/// `GET /api/snapshots?source=&key=` — one row by identity (#135-137).
/// Query params, not path segments, for `handlers/mod.rs`'s own reason
/// (module doc above): every source string contains a slash. `Device |
/// Ingest` (see `auth::permitted`): an evaluated-stream poller reads its own
/// previously-written cursor here so a restart resumes rather than replays
/// (ADR-0011) — the durable home `POST /api/snapshots` already gave it a
/// write path to, with no way back in. An `ingest` token is bound to the
/// row's `source`, exactly as on the write side; `token_source: None` means
/// a device token, which already reads every row through `GET /api/sweep`
/// and is let through unrestricted here too.
///
/// 404 for no such row, distinctly from a 400 for a missing/empty query
/// param — a poller's first-ever run (no cursor written yet) must be able
/// to tell "read the identity wrong" apart from "nothing here yet, this is
/// a legitimate start-from-scratch state."
pub fn get(
    query: Option<&str>,
    token_source: Option<&str>,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    let Some(source) = query_param(query, "source").filter(|s| !s.is_empty()) else {
        return Ok(error(400, "validation", "source is required"));
    };
    let Some(key) = query_param(query, "key").filter(|k| !k.is_empty()) else {
        return Ok(error(400, "validation", "key is required"));
    };
    if let Some(bound) = token_source {
        if bound != source {
            return Ok(empty_status(403));
        }
    }
    match select_snapshot(sql, source, key)? {
        Some(row) => Ok(json(200, &snapshot_from_row(&row)?)),
        None => Ok(error(404, "not_found", "no such snapshot")),
    }
}

fn select_snapshot(sql: &dyn Sql, source: &str, key: &str) -> Result<Option<Row>, SqlError> {
    Ok(sql
        .exec(
            "SELECT * FROM context_snapshots WHERE source = ? AND key = ?",
            &[
                SqlValue::Text(source.to_string()),
                SqlValue::Text(key.to_string()),
            ],
        )?
        .into_iter()
        .next())
}

pub(super) fn snapshot_from_row(row: &Row) -> Result<ContextSnapshot, SqlError> {
    let r = RowReader(row);
    Ok(ContextSnapshot {
        source: r.text("source")?,
        key: r.text("key")?,
        payload: r.text("payload")?,
        fetched_at: r.int("fetched_at")?,
        version: r.int("version")?,
    })
}
