//! `POST /api/alerts/healthchecks` (#893, `ingest` scope): Healthchecks.io's
//! webhook, translated onto `healthchecks/v1` alerts. A check that stops
//! checking in raises one alert keyed by the check, titled with its name
//! and carrying severity `high`; checking in again resolves it; stopping
//! again re-raises the same alert with a new raise time, so it rings again.
//!
//! It is a receiver beside `POST /api/alerts` rather than a body posted to
//! it because Healthchecks.io cannot fill an integer timestamp: its only
//! time placeholder, `$NOW`, is ISO 8601, and an up event has to carry a
//! `resolved_at`. So the server stamps both ends with its own clock. The
//! down event goes through `alerts::ingest_parsed` with `restamp_on_change`
//! (#120), which restamps `raised_at` exactly when the down clears a
//! resolution, and a repeated down (Healthchecks.io retries a failed call
//! up to twice) changes nothing and rings nothing. The up event writes
//! only `resolved_at`, through the same `alerts::resolve` the item-threshold
//! sweep uses, so title, severity and `raised_at` stay as the down left
//! them. No other source's behavior moves.
//!
//! ## The integration, exactly
//!
//! One Webhook integration in the Healthchecks.io project covers every
//! check. It authenticates with an `ingest` token bound to `healthchecks/v1`
//! (`scripts/mint-ingest-token.sh healthchecks-ingest healthchecks/v1
//! hummingbird-healthchecks-ingest`). Both the "goes down" and the "goes up"
//! halves are filled in identically:
//!
//! - **URL**: `POST` `https://hb.twinion.net/api/alerts/healthchecks`
//! - **Request Body**: `{"check": "$CODE", "name": $NAME_JSON, "status": "$STATUS"}`
//! - **Request Headers**: `Authorization: Bearer <the token>`
//!
//! `$NAME_JSON`, unquoted, is deliberate: it is the name already encoded as
//! a JSON string, so a name with a quote in it still parses. An unnamed
//! check is titled with its UUID rather than refused, so it still rings.
//!
//! Healthchecks.io has no per-integration "default for new checks" switch.
//! Adding the integration assigns it to every existing check in the
//! project. A check created in the dashboard gets every integration in the
//! project. A check created through the Management API gets them only when
//! the create call sends `"channels": "*"`.

use hummingbird_domain::{healthchecks_v1_key, AlertIngest, HealthchecksEvent, HEALTHCHECKS_V1};

use super::alerts::{find_by_identity, ingest_parsed, resolve};
use super::{empty_status, error, json, parse_body, ApiResponse};
use crate::sql::{Sql, SqlError};

/// The severity every Healthchecks.io down event raises at. A check that
/// stopped checking in means the app or the Mac is gone, and the rule
/// `healthchecks-high-urgent` rings it.
const SEVERITY: &str = "high";

/// `token_source` is the caller's bound source, as in `alerts::ingest`: a
/// token bound to anything but `healthchecks/v1` is a 403 with an empty
/// body before the body is read.
pub fn receive(
    body: Option<&str>,
    now_ms: i64,
    token_source: Option<&str>,
    token_origin: Option<&str>,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    if token_source != Some(HEALTHCHECKS_V1) {
        return Ok(empty_status(403));
    }
    let event: HealthchecksEvent = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if event.check.is_empty() {
        return Ok(error(400, "validation", "check must be non-empty"));
    }
    let source_key = healthchecks_v1_key(&event.check);
    match event.status.as_str() {
        "down" => {
            let title = if event.name.is_empty() { event.check } else { event.name };
            let ingest = AlertIngest {
                source: HEALTHCHECKS_V1.to_string(),
                source_key,
                subject_key: None,
                title,
                body: None,
                url: None,
                severity: Some(SEVERITY.to_string()),
                raised_at: None,
                resolved_at: None,
                expires_at: None,
                restamp_on_change: true,
            };
            ingest_parsed(ingest, now_ms, token_source, token_origin, sql)
        }
        "up" => {
            // An up for a check this authority never saw go down (the
            // integration was added mid-outage, or the down was lost) has
            // nothing to resolve, and writes nothing.
            let Some(alert) = find_by_identity(sql, HEALTHCHECKS_V1, &source_key)? else {
                return Ok(empty_status(204));
            };
            // Already resolved since its last raise: a retried up keeps the
            // first resolution. A dismissed alert is still resolved, so the
            // next down's restamp overtakes the dismissal and rings.
            if alert.resolved_at.is_some_and(|resolved| resolved >= alert.raised_at) {
                return Ok(json(200, &alert));
            }
            Ok(json(200, &resolve(sql, &alert, now_ms)?))
        }
        other => Ok(error(
            400,
            "validation",
            &format!("status must be `down` or `up`, not `{other}`"),
        )),
    }
}
