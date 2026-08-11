//! Parsing one Microsoft Graph delta-query response page — shared between
//! `mail_stream.rs` and `calendar_stream.rs`, because Graph's delta
//! protocol uses the exact same envelope for every resource collection
//! (`value`, `@odata.nextLink`, `@odata.deltaLink`), unlike the two Google
//! pollers, whose delta shapes (`gmail_poll::history`'s `history`/
//! `historyId` vs `calendar_poll::sync`'s `items`/`nextSyncToken`) differ
//! enough that #135/#136 each wrote their own. `gmail_poll::history`'s own
//! role, generalized.
//!
//! **Both `@odata.nextLink` and `@odata.deltaLink` are stored and replayed
//! as whole URLs**, not tokens extracted and re-assembled into a fresh
//! query — Graph's own documented usage pattern, and it is what keeps
//! `main.rs` from having to reconstruct query parameters (window bounds,
//! `$select`, folder scope) that only the *first* request in a sync needs
//! to state explicitly. A stored `deltaLink` is a complete, re-GETtable
//! request on its own.

use std::fmt;

/// Why a delta-query response body could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaError {
    NotJson(String),
}

impl fmt::Display for DeltaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeltaError::NotJson(m) => write!(f, "response is not JSON: {m}"),
        }
    }
}

/// One delta-query page. Each item is carried as its own compact JSON text
/// — `calendar_poll::sync::SyncPage`'s own shape — rather than a typed
/// struct here, so this module needs no knowledge of whether the
/// collection is mail or calendar. `next_link` is present on every page
/// except the last; `delta_link` is present only on the last page of a
/// sync (incremental or full) — Graph's own signal that pagination is
/// complete and the sync is caught up.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeltaPage {
    pub raw_items: Vec<String>,
    pub next_link: Option<String>,
    pub delta_link: Option<String>,
}

/// Parses one Graph delta-query response body.
pub fn parse_delta_page(json: &str) -> Result<DeltaPage, DeltaError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| DeltaError::NotJson(e.to_string()))?;
    let object = value.as_object().ok_or_else(|| DeltaError::NotJson("not an object".into()))?;

    let raw_items = object
        .get("value")
        .and_then(|v| v.as_array())
        .map(|items| items.iter().map(|item| item.to_string()).collect())
        .unwrap_or_default();
    let next_link = object.get("@odata.nextLink").and_then(|v| v.as_str()).map(str::to_string);
    let delta_link = object.get("@odata.deltaLink").and_then(|v| v.as_str()).map(str::to_string);

    Ok(DeltaPage { raw_items, next_link, delta_link })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_with_items_collects_their_raw_json() {
        let json = r#"{
            "value": [{"id": "e1"}, {"id": "e2"}],
            "@odata.deltaLink": "https://graph.microsoft.com/v1.0/.../delta?$deltatoken=final"
        }"#;
        let page = parse_delta_page(json).unwrap();
        assert_eq!(page.raw_items.len(), 2);
        assert!(page.raw_items[0].contains("\"e1\""));
        assert_eq!(page.delta_link.as_deref(), Some("https://graph.microsoft.com/v1.0/.../delta?$deltatoken=final"));
        assert_eq!(page.next_link, None);
    }

    #[test]
    fn a_page_with_no_items_is_empty_not_malformed() {
        let json = r#"{"@odata.deltaLink": "https://x/delta?$deltatoken=final"}"#;
        let page = parse_delta_page(json).expect("nothing changed is not an error");
        assert!(page.raw_items.is_empty());
        assert!(page.delta_link.is_some());
    }

    #[test]
    fn a_next_link_is_carried_through() {
        let json = r#"{"value": [], "@odata.nextLink": "https://x/delta?$skiptoken=abc"}"#;
        let page = parse_delta_page(json).unwrap();
        assert_eq!(page.next_link.as_deref(), Some("https://x/delta?$skiptoken=abc"));
        assert_eq!(page.delta_link, None, "not the final page");
    }

    #[test]
    fn not_json_is_named() {
        assert!(matches!(parse_delta_page("nope"), Err(DeltaError::NotJson(_))));
    }
}
