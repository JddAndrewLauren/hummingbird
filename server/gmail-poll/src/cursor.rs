//! The `historyId` cursor's durable home (ADR-0011: "each stream keeps a
//! per-source delta cursor... the same cursor concept the sync engine
//! already uses"). Stored as an ordinary `context_snapshots` row under this
//! poller's own bound source — the exact lane `server/city-waste` already
//! uses for its snapshot, generalized here to internal state no pane ever
//! reads: `GET /api/snapshots?source=&key=` (#135, added alongside this
//! poller for the read-back a restart needs) and the existing
//! `POST /api/snapshots` are the write/read pair, both reachable by the
//! same `ingest` token this poller already holds for `POST /api/alerts`.

use hummingbird_domain::{SnapshotEnvelope, GMAIL_V1};

/// This poller's bound source — reused for both the alert lane and the
/// cursor snapshot, exactly as `city-waste/v2` is reused for both its
/// snapshot and its alert (`sources.rs`'s own "a source may of course be
/// both").
pub const SOURCE: &str = GMAIL_V1;

/// The one `context_snapshots.key` this poller owns for its cursor.
pub const CURSOR_KEY: &str = "cursor";

/// This payload shape's own versioned name (ADR-0015's envelope
/// `schema`) — distinct from `SOURCE`, since one names the payload shape
/// and the other names the feed, and nothing couples them.
pub const CURSOR_SCHEMA: &str = "gmail-cursor/v1";

/// How often this poller says it runs — must match `crontab`'s entry for
/// `hummingbird-gmail-poll` (#774 moved this off `gmail-poll.yml`'s Actions
/// `schedule:`), the same discipline `city-waste::body::POLLED_EVERY_MS`
/// documents.
pub const POLLED_EVERY_MS: i64 = 15 * 60 * 1000;

/// Builds the `POST /api/snapshots` payload for a fresh `historyId`.
pub fn envelope(history_id: &str) -> serde_json::Value {
    serde_json::json!({
        "schema": CURSOR_SCHEMA,
        "polled_every_ms": POLLED_EVERY_MS,
        "body": { "history_id": history_id },
    })
}

/// Why a stored cursor row could not be read back as a `historyId`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorError {
    Envelope(String),
    BodyNotJson,
    MissingHistoryId,
}

impl std::fmt::Display for CursorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CursorError::Envelope(reason) => write!(f, "cursor envelope: {reason}"),
            CursorError::BodyNotJson => write!(f, "cursor body is not JSON"),
            CursorError::MissingHistoryId => write!(f, "cursor body has no `history_id`"),
        }
    }
}

/// Reads a `historyId` back out of a stored snapshot's `payload` text
/// (`GET /api/snapshots`'s `payload` field, exactly as written by
/// [`envelope`]).
pub fn parse_cursor(payload: &str) -> Result<String, CursorError> {
    let env = SnapshotEnvelope::parse(payload).map_err(|p| CursorError::Envelope(p.to_string()))?;
    let body: serde_json::Value =
        serde_json::from_str(&env.body).map_err(|_| CursorError::BodyNotJson)?;
    body.get("history_id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or(CursorError::MissingHistoryId)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_cursor_reads_back_the_same_history_id() {
        let payload = envelope("998877").to_string();
        assert_eq!(parse_cursor(&payload), Ok("998877".to_string()));
    }

    #[test]
    fn the_envelope_this_poller_builds_passes_the_authoritys_own_parse() {
        let payload = envelope("1").to_string();
        let parsed = SnapshotEnvelope::parse(&payload).expect("the authority accepts what this poller writes");
        assert_eq!(parsed.schema, CURSOR_SCHEMA);
        assert_eq!(parsed.polled_every_ms, Some(POLLED_EVERY_MS));
    }

    #[test]
    fn a_broken_envelope_is_named_not_silently_treated_as_no_cursor() {
        assert!(matches!(parse_cursor("not json"), Err(CursorError::Envelope(_))));
    }

    #[test]
    fn a_body_with_no_history_id_is_named() {
        let payload = r#"{"schema": "gmail-cursor/v1", "body": {"oops": true}}"#;
        assert_eq!(parse_cursor(payload), Err(CursorError::MissingHistoryId));
    }
}
