//! Routing, parsing and the helpers every entity handler shares. Everything
//! here is a pure function of the request, the injected clock and the
//! [`Sql`] seam — the shim adds nothing but transport.

mod blocked_by;
mod changes;
mod fog;
mod items;
mod projects;
mod routes;
mod steps;

use hummingbird_domain::{ApiError, ConflictResponse, VERSION_CONFLICT};
use serde::Serialize;

use crate::sql::{Sql, SqlError, SqlValue};

/// The transport-agnostic request: the shim maps `worker::Request` onto
/// this, tests build it directly.
pub struct ApiRequest<'a> {
    pub method: &'a str,
    /// Path only, no query string — e.g. `/api/items/uuid-1`.
    pub path: &'a str,
    /// The raw query string, without the `?`.
    pub query: Option<&'a str>,
    pub body: Option<&'a str>,
}

/// Status + JSON body; every response body is JSON.
#[derive(Debug, PartialEq)]
pub struct ApiResponse {
    pub status: u16,
    pub body: String,
}

/// The one entry point. `now_ms` is injected (the shim passes the worker
/// clock) — nothing in this crate reads a clock of its own.
pub fn handle(req: &ApiRequest, now_ms: i64, sql: &dyn Sql) -> ApiResponse {
    let result = route(req, now_ms, sql);
    result.unwrap_or_else(|e| error(500, "internal", &e.message))
}

fn route(req: &ApiRequest, now_ms: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let Some(rest) = req.path.strip_prefix("/api/") else {
        return Ok(error(404, "not_found", "no such route"));
    };
    let segments: Vec<&str> = rest.split('/').collect();
    match (req.method, segments.as_slice()) {
        ("POST", ["items"]) => items::create(req.body, now_ms, sql),
        ("PATCH", ["items", id]) if !id.is_empty() => items::patch(id, req.body, now_ms, sql),
        ("POST", ["projects"]) => projects::create(req.body, now_ms, sql),
        ("PATCH", ["projects", id]) if !id.is_empty() => {
            projects::patch(id, req.body, now_ms, sql)
        }
        ("PATCH", ["routes", project_id]) if !project_id.is_empty() => {
            routes::patch(project_id, req.body, now_ms, sql)
        }
        ("POST", ["fog"]) => fog::create(req.body, now_ms, sql),
        ("PATCH", ["fog", id]) if !id.is_empty() => fog::patch(id, req.body, now_ms, sql),
        ("POST", ["steps"]) => steps::create(req.body, now_ms, sql),
        ("PATCH", ["steps", id]) if !id.is_empty() => steps::patch(id, req.body, now_ms, sql),
        ("POST", ["blocked_by"]) => blocked_by::create(req.body, now_ms, sql),
        ("PATCH", ["blocked_by", item_id, blocker_id])
            if !item_id.is_empty() && !blocker_id.is_empty() =>
        {
            blocked_by::patch(item_id, blocker_id, req.body, now_ms, sql)
        }
        ("GET", ["changes"]) => changes::changes(req.query, sql),
        // A known collection or entity path with the wrong method is a 405;
        // anything else falls through to 404.
        (_, ["items" | "projects" | "fog" | "steps" | "blocked_by" | "changes"]) => {
            Ok(method_not_allowed())
        }
        (_, ["items" | "projects" | "routes" | "fog" | "steps", id]) if !id.is_empty() => {
            Ok(method_not_allowed())
        }
        (_, ["blocked_by", item_id, blocker_id])
            if !item_id.is_empty() && !blocker_id.is_empty() =>
        {
            Ok(method_not_allowed())
        }
        _ => Ok(error(404, "not_found", "no such route")),
    }
}

// --------------------------------------------------------------- helpers

fn parse_body<T: serde::de::DeserializeOwned>(body: Option<&str>) -> Result<T, ApiResponse> {
    let body = body
        .filter(|b| !b.is_empty())
        .ok_or_else(|| error(400, "bad_json", "a JSON body is required"))?;
    serde_json::from_str(body).map_err(|e| error(400, "bad_json", &e.to_string()))
}

fn query_param<'a>(query: Option<&'a str>, name: &str) -> Option<&'a str> {
    query?
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

fn read_meta_version(sql: &dyn Sql) -> Result<i64, SqlError> {
    sql.exec("SELECT version FROM meta WHERE id = 1", &[])?
        .first()
        .and_then(|r| r.get("version").and_then(SqlValue::as_i64))
        .ok_or_else(|| SqlError {
            message: "meta row missing — init_schema not run".into(),
        })
}

fn write_meta_version(sql: &dyn Sql, version: i64) -> Result<(), SqlError> {
    sql.exec(
        "UPDATE meta SET version = ? WHERE id = 1",
        &[SqlValue::Integer(version)],
    )?;
    Ok(())
}

/// The 409: a stale `expected_version` write is answered with the current
/// entity so the client can rebase (ADR-0008).
fn conflict<T: Serialize>(current: &T) -> ApiResponse {
    json(
        409,
        &ConflictResponse {
            error: VERSION_CONFLICT.to_string(),
            current,
        },
    )
}

fn json<T: Serialize>(status: u16, value: &T) -> ApiResponse {
    ApiResponse {
        status,
        body: serde_json::to_string(value).expect("DTOs serialize"),
    }
}

fn error(status: u16, code: &str, message: &str) -> ApiResponse {
    json(
        status,
        &ApiError {
            error: code.to_string(),
            message: message.to_string(),
        },
    )
}

fn method_not_allowed() -> ApiResponse {
    error(405, "method_not_allowed", "wrong method for this route")
}
