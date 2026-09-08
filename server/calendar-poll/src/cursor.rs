//! The two `context_snapshots` rows this poller owns, both under its own
//! bound source (`gmail_poll::cursor`'s own reason, generalized): the
//! sync-token delta cursor (ADR-0011: "each stream keeps a per-source delta
//! cursor") and the `busy_now` gauge (this issue's own second job). One
//! source, two keys — `GET`/`POST /api/snapshots` (#135) is the read/write
//! pair for both, reachable by the same `ingest` token this poller already
//! holds for `POST /api/alerts`.

use hummingbird_domain::{SnapshotEnvelope, GOOGLE_CALENDAR_V1};

/// This poller's bound source — reused for the alert lane, the cursor
/// snapshot, AND the busy snapshot (`sources.rs`'s "a source may of course
/// be both", extended here to a source being three things at once: an
/// alert source and two different snapshot keys).
pub const SOURCE: &str = GOOGLE_CALENDAR_V1;

/// How often this poller says it runs — must match `crontab`'s entry for
/// `hummingbird-calendar-poll` (#774 moved this off `calendar-poll.yml`'s
/// Actions `schedule:`), the same discipline
/// `gmail_poll::cursor::POLLED_EVERY_MS` and `city_waste::body::POLLED_EVERY_MS`
/// document.
pub const POLLED_EVERY_MS: i64 = 15 * 60 * 1000;

// --------------------------------------------------------------- cursor

/// The one `context_snapshots.key` this poller owns for its sync-token
/// cursor.
pub const CURSOR_KEY: &str = "cursor";

/// This payload shape's own versioned name (ADR-0015's envelope `schema`).
pub const CURSOR_SCHEMA: &str = "google-calendar-cursor/v1";

/// Builds the `POST /api/snapshots` payload for a fresh `syncToken`.
pub fn cursor_envelope(sync_token: &str) -> serde_json::Value {
    serde_json::json!({
        "schema": CURSOR_SCHEMA,
        "polled_every_ms": POLLED_EVERY_MS,
        "body": { "sync_token": sync_token },
    })
}

/// Why a stored cursor row could not be read back as a `syncToken`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorError {
    Envelope(String),
    BodyNotJson,
    MissingSyncToken,
}

impl std::fmt::Display for CursorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CursorError::Envelope(reason) => write!(f, "cursor envelope: {reason}"),
            CursorError::BodyNotJson => write!(f, "cursor body is not JSON"),
            CursorError::MissingSyncToken => write!(f, "cursor body has no `sync_token`"),
        }
    }
}

/// Reads a `syncToken` back out of a stored snapshot's `payload` text
/// (`GET /api/snapshots`'s `payload` field, exactly as written by
/// [`cursor_envelope`]).
pub fn parse_cursor(payload: &str) -> Result<String, CursorError> {
    let env = SnapshotEnvelope::parse(payload).map_err(|p| CursorError::Envelope(p.to_string()))?;
    let body: serde_json::Value =
        serde_json::from_str(&env.body).map_err(|_| CursorError::BodyNotJson)?;
    body.get("sync_token")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or(CursorError::MissingSyncToken)
}

// ----------------------------------------------------------- busy_now

/// The one `context_snapshots.key` this poller owns for the busy gauge.
pub const BUSY_KEY: &str = "busy_now";

/// This payload shape's own versioned name.
pub const BUSY_SCHEMA: &str = "google-calendar-busy/v1";

/// Builds the `POST /api/snapshots` payload for the busy window — replaced
/// wholesale each poll, per the brief. `window` is `None` when nothing is
/// busy right now; the body still writes (a fresh `fetched_at`, the poller
/// alive), it just carries no boundaries — never a row that vanishes.
pub fn busy_envelope(window: Option<crate::busy::BusyWindow>) -> serde_json::Value {
    let body = match window {
        Some(w) => serde_json::json!({ "start_ms": w.start_ms, "end_ms": w.end_ms }),
        None => serde_json::json!({ "start_ms": null, "end_ms": null }),
    };
    serde_json::json!({
        "schema": BUSY_SCHEMA,
        "polled_every_ms": POLLED_EVERY_MS,
        "body": body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::busy::BusyWindow;

    #[test]
    fn a_written_cursor_reads_back_the_same_sync_token() {
        let payload = cursor_envelope("CAoQ...token").to_string();
        assert_eq!(parse_cursor(&payload), Ok("CAoQ...token".to_string()));
    }

    #[test]
    fn the_cursor_envelope_this_poller_builds_passes_the_authoritys_own_parse() {
        let payload = cursor_envelope("t").to_string();
        let parsed = SnapshotEnvelope::parse(&payload).expect("the authority accepts what this poller writes");
        assert_eq!(parsed.schema, CURSOR_SCHEMA);
        assert_eq!(parsed.polled_every_ms, Some(POLLED_EVERY_MS));
    }

    #[test]
    fn a_broken_cursor_envelope_is_named_not_silently_treated_as_no_cursor() {
        assert!(matches!(parse_cursor("not json"), Err(CursorError::Envelope(_))));
    }

    #[test]
    fn a_cursor_body_with_no_sync_token_is_named() {
        let payload = r#"{"schema": "google-calendar-cursor/v1", "body": {"oops": true}}"#;
        assert_eq!(parse_cursor(payload), Err(CursorError::MissingSyncToken));
    }

    #[test]
    fn the_busy_envelope_this_poller_builds_passes_the_authoritys_own_parse() {
        let payload = busy_envelope(Some(BusyWindow { start_ms: 1000, end_ms: 2000 })).to_string();
        let parsed = SnapshotEnvelope::parse(&payload).expect("the authority accepts what this poller writes");
        assert_eq!(parsed.schema, BUSY_SCHEMA);
    }

    #[test]
    fn a_not_busy_window_still_writes_a_row_with_null_boundaries() {
        let value = busy_envelope(None);
        assert_eq!(value["body"]["start_ms"], serde_json::Value::Null);
        assert_eq!(value["body"]["end_ms"], serde_json::Value::Null);
    }
}
