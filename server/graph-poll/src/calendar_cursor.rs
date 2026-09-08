//! The `context_snapshots` row `graph-calendar-poll` owns: its delta
//! cursor, under its own bound source — `mail_cursor.rs`'s own pattern,
//! for `m365-calendar/v1`. Unlike `google-calendar/v1` (#136), this source
//! owns no `busy_now` snapshot — #137's brief names only the evaluated
//! stream, not a busy gauge for the M365 calendar leg.

use hummingbird_domain::{SnapshotEnvelope, M365_CALENDAR_V1};

pub const SOURCE: &str = M365_CALENDAR_V1;

/// Must match `crontab`'s entry for `graph-calendar-poll` (#774 moved this
/// off `graph-calendar-poll.yml`'s Actions `schedule:`).
pub const POLLED_EVERY_MS: i64 = 15 * 60 * 1000;

pub const CURSOR_KEY: &str = "cursor";
pub const CURSOR_SCHEMA: &str = "m365-calendar-cursor/v1";

pub fn cursor_envelope(delta_link: &str) -> serde_json::Value {
    serde_json::json!({
        "schema": CURSOR_SCHEMA,
        "polled_every_ms": POLLED_EVERY_MS,
        "body": { "delta_link": delta_link },
    })
}

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
    }

    #[test]
    fn a_broken_cursor_envelope_is_named_not_silently_treated_as_no_cursor() {
        assert!(matches!(parse_cursor("not json"), Err(CursorError::Envelope(_))));
    }

    #[test]
    fn a_cursor_body_with_no_delta_link_is_named() {
        let payload = r#"{"schema": "m365-calendar-cursor/v1", "body": {"oops": true}}"#;
        assert_eq!(parse_cursor(payload), Err(CursorError::MissingDeltaLink));
    }
}
