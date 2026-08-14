//! Routing, parsing and the helpers every entity handler shares. Everything
//! here is a pure function of the request, the injected clock and the
//! [`Sql`] seam — the shim adds nothing but transport.

mod admin_tokens;
mod alerts;
mod auth;
mod blocked_by;
mod changes;
mod fog;
mod grills;
mod items;
mod projects;
pub(crate) mod push_targets;
mod routes;
mod rules;
mod settings;
mod skills;
mod snapshots;
mod steps;

// Re-exported for #138's sweep module, which mints
// `item-threshold/v1` alerts and reads item/rule rows through these exact
// functions rather than a parallel implementation of either.
pub(crate) use alerts::find_by_identity as find_alert_by_identity;
pub(crate) use alerts::find_live_by_source as find_live_alerts_by_source;
pub(crate) use alerts::resolve as resolve_alert;
pub(crate) use alerts::upsert as upsert_alert;
pub(crate) use grills::grill_without_transcript_from_row;
pub(crate) use items::item_from_row;
pub(crate) use rules::load_enabled as load_enabled_rules;
pub(crate) use rules::rule_from_row;

// Re-exported for `delivery.rs` (#139/#220), so its id derivation shares
// the one sha256+hex spelling `auth` and `alerts::deterministic_id` already
// use rather than hand-rolling a third.
pub(crate) use auth::sha256_hex;

use hummingbird_domain::{ApiError, ConflictResponse, VERSION_CONFLICT};
use serde::Serialize;

use crate::delivery::DeliveryOutcome;
use crate::entropy::Entropy;
use crate::sql::{Sql, SqlError, SqlValue};

/// The transport-agnostic request: the shim maps `worker::Request` onto
/// this, tests build it directly.
pub struct ApiRequest<'a> {
    pub method: &'a str,
    /// Path only, no query string — e.g. `/api/items/uuid-1`. Segments are
    /// matched exactly as received (no percent-decoding): entity ids and
    /// settings keys must be URL-safe literals — an encoded character is
    /// stored and matched verbatim.
    pub path: &'a str,
    /// The raw query string, without the `?`.
    pub query: Option<&'a str>,
    pub body: Option<&'a str>,
    /// The raw `Authorization` header value, if any. Parsing lives here in
    /// the pure crate, not the shim.
    pub authorization: Option<&'a str>,
}

/// Status + JSON body. Every response body is JSON — except the 401/403,
/// whose bodies are empty by design (clean status, no leakage).
#[derive(Debug, PartialEq)]
pub struct ApiResponse {
    pub status: u16,
    pub body: String,
    /// #255's inline evaluate-then-deliver hook: `POST /api/alerts` is the
    /// second caller of `deliver` (`sweep_tick`'s DO alarm is the first),
    /// and — unlike the alarm's own `tick_result` return — an HTTP handler
    /// has no separate return channel of its own, so whatever it logged
    /// rides back on the response the caller (the worker shim) already
    /// has to unwrap. Empty for every other route: nothing else in this
    /// crate calls `deliver`.
    pub deliveries: Vec<DeliveryOutcome>,
}

/// Everything injected around a request: the clock, the `ADMIN_SECRET`
/// Worker secret (`None` = admin routes fail closed), and the entropy
/// source for token minting. Nothing in this crate reads a clock or an
/// environment of its own.
pub struct HandleContext<'a> {
    pub now_ms: i64,
    pub admin_secret: Option<&'a str>,
    pub entropy: &'a dyn Entropy,
}

/// The one entry point.
pub fn handle(req: &ApiRequest, ctx: &HandleContext, sql: &dyn Sql) -> ApiResponse {
    let result = route(req, ctx, sql);
    result.unwrap_or_else(|e| error(500, "internal", &e.message))
}

fn route(req: &ApiRequest, ctx: &HandleContext, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let Some(rest) = req.path.strip_prefix("/api/") else {
        return Ok(error(404, "not_found", "no such route"));
    };
    let segments: Vec<&str> = rest.split('/').collect();

    // The admin lane authenticates against ADMIN_SECRET, never the tokens
    // table.
    if segments.first() == Some(&"admin") {
        if !auth::admin_ok(req.authorization, ctx.admin_secret) {
            return Ok(empty_status(401));
        }
        return match (req.method, segments.as_slice()) {
            ("POST", ["admin", "tokens"]) => admin_tokens::mint(req.body, ctx, sql),
            ("GET", ["admin", "tokens"]) => admin_tokens::list(sql),
            ("DELETE", ["admin", "tokens", id]) if !id.is_empty() => {
                admin_tokens::revoke(id, ctx.now_ms, sql)
            }
            (_, ["admin", "tokens"]) => Ok(method_not_allowed()),
            (_, ["admin", "tokens", id]) if !id.is_empty() => Ok(method_not_allowed()),
            _ => Ok(error(404, "not_found", "no such route")),
        };
    }

    // Everything else authenticates first — an unauthenticated caller never
    // learns the route map — then passes the scope matrix before routing.
    let Some(principal) = auth::authenticate(req.authorization, ctx.now_ms, sql)? else {
        return Ok(empty_status(401));
    };
    if !auth::permitted(principal.scope, req.method, &segments) {
        return Ok(empty_status(403));
    }

    let now_ms = ctx.now_ms;
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
        ("PUT", ["settings", key]) if !key.is_empty() => {
            settings::put(key, req.body, now_ms, sql)
        }
        ("POST", ["alerts"]) => alerts::ingest(req.body, now_ms, principal.source.as_deref(), sql),
        ("POST", ["snapshots"]) => {
            snapshots::ingest(req.body, now_ms, principal.source.as_deref(), sql)
        }
        ("PATCH", ["alerts", id]) if !id.is_empty() => alerts::dismiss(id, req.body, now_ms, sql),
        ("POST", ["rules"]) => rules::create(req.body, now_ms, sql),
        ("PATCH", ["rules", id]) if !id.is_empty() => rules::patch(id, req.body, now_ms, sql),
        ("POST", ["push_targets"]) => push_targets::register(req.body, now_ms, sql),
        ("DELETE", ["push_targets", id]) if !id.is_empty() => {
            push_targets::revoke(id, now_ms, sql)
        }
        ("POST", ["grills"]) => grills::create(req.body, now_ms, sql),
        ("GET", ["grills", id]) if !id.is_empty() => grills::get(id, sql),
        ("GET", ["settings", key]) if !key.is_empty() => settings::get(key, sql),
        ("GET", ["rules"]) => rules::list(principal.source.as_deref(), sql),
        ("GET", ["snapshots"]) => {
            snapshots::get(req.query, principal.source.as_deref(), sql)
        }
        ("GET", ["changes"]) => changes::changes(req.query, now_ms, sql),
        ("GET", ["sweep"]) => changes::sweep(now_ms, sql),
        // The skill-runner proxy's authorization verdict (#273): the DO
        // answers only whether the caller may run a skill; the shim
        // performs the egress above this dispatch. The body never reaches
        // here — the preflight is deliberately bodiless.
        ("POST", ["skills", "run"]) => Ok(skills::run_verdict()),
        // A known collection or entity path with the wrong method is a 405;
        // anything else falls through to 404.
        (
            _,
            ["items" | "projects" | "fog" | "steps" | "blocked_by" | "alerts" | "rules"
                | "push_targets" | "snapshots" | "changes" | "sweep" | "grills"],
        ) => Ok(method_not_allowed()),
        (
            _,
            ["items" | "projects" | "routes" | "fog" | "steps" | "settings" | "alerts" | "rules"
                | "push_targets" | "grills", id],
        ) if !id.is_empty() =>
        {
            Ok(method_not_allowed())
        }
        (_, ["blocked_by", item_id, blocker_id])
            if !item_id.is_empty() && !blocker_id.is_empty() =>
        {
            Ok(method_not_allowed())
        }
        // `["skills", "run"]` is a known path whose second segment is a
        // literal, not an id, so neither collection arm above covers it —
        // without this it would 404 on a `GET`, unlike every other known
        // route.
        (_, ["skills", "run"]) => Ok(method_not_allowed()),
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
        deliveries: Vec::new(),
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

/// The 401/403 shape: a clean status and nothing else.
fn empty_status(status: u16) -> ApiResponse {
    ApiResponse {
        status,
        body: String::new(),
        deliveries: Vec::new(),
    }
}
