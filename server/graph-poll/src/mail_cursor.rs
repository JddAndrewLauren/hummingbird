//! The `context_snapshots` row `graph-mail-poll` owns: its delta cursor
//! (ADR-0011: "each stream keeps a per-source delta cursor"), under its own
//! bound source — `gmail_poll::cursor`'s own reason. `GET`/`POST
//! /api/snapshots` (#135) is the read/write pair, reachable by the same
//! `ingest` token this binary already holds for `POST /api/alerts`.

use hummingbird_domain::{SnapshotEnvelope, M365_MAIL_V1};

/// This binary's bound source — reused for the alert lane and the cursor
/// snapshot (`sources.rs`'s "a source may of course be both").
pub const SOURCE: &str = M365_MAIL_V1;

/// How often this poller says it runs — must match
/// `.github/workflows/graph-mail-poll.yml`'s cron, the same discipline the
/// other two pollers' `POLLED_EVERY_MS` document.
pub const POLLED_EVERY_MS: i64 = 15 * 60 * 1000;

/// The one `context_snapshots.key` this binary owns.
pub const CURSOR_KEY: &str = "cursor";

/// This payload shape's own versioned name (ADR-0015's envelope `schema`).
pub const CURSOR_SCHEMA: &str = "m365-mail-cursor/v1";

/// Builds the `POST /api/snapshots` payload for a fresh `deltaLink`.
pub fn cursor_envelope(delta_link: &str) -> serde_json::Value {
    serde_json::json!({
        "schema": CURSOR_SCHEMA,
        "polled_every_ms": POLLED_EVERY_MS,
        "body": { "delta_link": delta_link },
    })
}

/// Why a stored cursor row could not be read back as a `deltaLink`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorError {
    Envelope(String),
    BodyNotJson,
    MissingDeltaLink,
}

impl std::fmt::Display for CursorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CursorError::Envelope(reason) => write!(f, "cursor envelope: {reason}"),
            CursorError::BodyNotJson => write!(f, "cursor body is not JSON"),
            CursorError::MissingDeltaLink => write!(f, "cursor body has no `delta_link`"),
        }
    }
}

/// Reads a `deltaLink` back out of a stored snapshot's `payload` text
/// (`GET /api/snapshots`'s `payload` field, exactly as written by
/// [`cursor_envelope`]).
pub fn parse_cursor(payload: &str) -> Result<String, CursorError> {
    let env = SnapshotEnvelope::parse(payload).map_err(|p| CursorError::Envelope(p.to_string()))?;
    let body: serde_json::Value = serde_json::from_str(&env.body).map_err(|_| CursorError::BodyNotJson)?;
    body.get("delta_link").and_then(|v| v.as_str()).map(str::to_string).ok_or(CursorError::MissingDeltaLink)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_cursor_reads_back_the_same_delta_link() {
        let payload = cursor_envelope("https://graph.microsoft.com/v1.0/.../delta?$deltatoken=t").to_string();
        assert_eq!(
            parse_cursor(&payload),
            Ok("https://graph.microsoft.com/v1.0/.../delta?$deltatoken=t".to_string())
        );
    }

    #[test]
    fn the_cursor_envelope_this_poller_builds_passes_the_authoritys_own_parse() {
        let payload = cursor_envelope("https://x/delta?$deltatoken=t").to_string();
        let parsed = SnapshotEnvelope::parse(&payload).expect("the authority accepts what this poller writes");
        assert_eq!(parsed.schema, CURSOR_SCHEMA);
        assert_eq!(parsed.polled_every_ms, Some(POLLED_EVERY_MS));
    }

    #[test]
    fn a_broken_cursor_envelope_is_named_not_silently_treated_as_no_cursor() {
        assert!(matches!(parse_cursor("not json"), Err(CursorError::Envelope(_))));
    }

    #[test]
    fn a_cursor_body_with_no_delta_link_is_named() {
        let payload = r#"{"schema": "m365-mail-cursor/v1", "body": {"oops": true}}"#;
        assert_eq!(parse_cursor(payload), Err(CursorError::MissingDeltaLink));
    }
}
