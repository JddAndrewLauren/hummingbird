//! Parsing Google Calendar's `events.list` response — the delta half of
//! ADR-0011's "per-source delta cursor" (`gmail_poll::history`'s own role,
//! for `syncToken` rather than `historyId`). The incremental shape (a
//! `syncToken` query) and the resync shape (a bounded `updatedMin` query,
//! no `syncToken`) share this exact response body; only the *request*
//! differs, `main.rs`'s job. Google signals an aged-out `syncToken` with an
//! HTTP 410 — `main.rs`'s job to notice, never this module's, which only
//! ever sees a 200 body.

use std::fmt;

/// Why an `events.list` body could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncError {
    NotJson(String),
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncError::NotJson(m) => write!(f, "response is not JSON: {m}"),
        }
    }
}

/// One `events.list` page. Each item is carried as its own compact JSON
/// text — [`crate::calendar_event::parse_calendar_event`]'s own input
/// shape — rather than a typed struct here, so this module needs no
/// knowledge of the event body at all. `next_sync_token` is present only on
/// the final page of a sync (incremental or full) — Google's own signal
/// that pagination is complete and the sync is caught up.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncPage {
    pub raw_events: Vec<String>,
    pub next_page_token: Option<String>,
    pub next_sync_token: Option<String>,
}

/// Parses one `events.list` response body.
pub fn parse_events_list(json: &str) -> Result<SyncPage, SyncError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| SyncError::NotJson(e.to_string()))?;
    let object = value.as_object().ok_or_else(|| SyncError::NotJson("not an object".into()))?;

    let raw_events = object
        .get("items")
        .and_then(|v| v.as_array())
        .map(|items| items.iter().map(|item| item.to_string()).collect())
        .unwrap_or_default();
    let next_page_token = object.get("nextPageToken").and_then(|v| v.as_str()).map(str::to_string);
    let next_sync_token = object.get("nextSyncToken").and_then(|v| v.as_str()).map(str::to_string);

    Ok(SyncPage { raw_events, next_page_token, next_sync_token })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_with_items_collects_their_raw_json() {
        let json = r#"{
            "items": [{"id": "e1", "status": "confirmed"}, {"id": "e2", "status": "confirmed"}],
            "nextSyncToken": "tok-final"
        }"#;
        let page = parse_events_list(json).unwrap();
        assert_eq!(page.raw_events.len(), 2);
        assert!(page.raw_events[0].contains("\"e1\""));
        assert_eq!(page.next_sync_token.as_deref(), Some("tok-final"));
        assert_eq!(page.next_page_token, None);
    }

    #[test]
    fn a_page_with_no_items_is_empty_not_malformed() {
        let json = r#"{"nextSyncToken": "tok-final"}"#;
        let page = parse_events_list(json).expect("nothing changed is not an error");
        assert!(page.raw_events.is_empty());
        assert_eq!(page.next_sync_token.as_deref(), Some("tok-final"));
    }

    #[test]
    fn a_next_page_token_is_carried_through() {
        let json = r#"{"items": [], "nextPageToken": "tok-abc"}"#;
        let page = parse_events_list(json).unwrap();
        assert_eq!(page.next_page_token.as_deref(), Some("tok-abc"));
        assert_eq!(page.next_sync_token, None, "not the final page");
    }

    #[test]
    fn not_json_is_named() {
        assert!(matches!(parse_events_list("nope"), Err(SyncError::NotJson(_))));
    }
}
