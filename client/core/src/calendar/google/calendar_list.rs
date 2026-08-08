//! [`list_calendars`]: the options #73's calendar picker offers.
//!
//! Not part of #71's event-fetching adapter (`calendars/{id}/events`) — this
//! is Google's separate `calendarList` endpoint, needed only so the picker
//! has real options to choose from. The same `calendar.readonly` scope covers
//! it, so no extra consent is ever requested for this call.
//!
//! It lives here, in the core, rather than in the web host that consumes it,
//! because ADR-0003 puts every authenticated HTTP request behind one
//! `reqwest` path shared by all four clients: an Android or iPad picker needs
//! exactly this list, and a browser `fetch` in `client/web` would be the
//! second HTTP path the ADR exists to prevent.

use std::fmt;

use super::transport::{CalendarListTransport, TransportError};

/// One selectable calendar. `summary` is what the picker labels the entry
/// with; Google may omit it, in which case the id stands in — an unlabeled
/// checkbox is worse than a technical one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CalendarListEntry {
    pub id: String,
    pub summary: String,
}

#[derive(Debug, serde::Deserialize)]
struct RawCalendarListItem {
    id: Option<String>,
    summary: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct RawCalendarListPage {
    #[serde(default)]
    items: Vec<RawCalendarListItem>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

/// Google's `calendarList.list` caps a page at 100 entries (its default
/// `maxResults`) and hands back a `nextPageToken` when there are more.
/// Stopping at page one would make every calendar past the first hundred
/// unselectable — invisible in the picker rather than visibly unavailable,
/// which is the worse failure.
const MAX_PAGES: usize = 20;

#[derive(Debug)]
pub enum CalendarListError {
    Transport(TransportError),
    InvalidResponse(String),
}

impl CalendarListError {
    /// Whether Google rejected the credential itself (HTTP 401), the one
    /// failure a retry against the same token cannot fix.
    pub fn is_unauthorized(&self) -> bool {
        match self {
            CalendarListError::Transport(source) => source.is_unauthorized(),
            CalendarListError::InvalidResponse(_) => false,
        }
    }
}

impl fmt::Display for CalendarListError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CalendarListError::Transport(source) => write!(f, "transport error: {source}"),
            CalendarListError::InvalidResponse(message) => {
                write!(f, "invalid response: {message}")
            }
        }
    }
}

impl std::error::Error for CalendarListError {}

/// Fetches every page of the signed-in user's calendar list.
///
/// The page bound and the repeated-token check guard the same
/// misbehaving-server case [`super::adapter::fetch_calendar_snapshot`] does,
/// but stop rather than fail: the picker's options are a UX nicety and never
/// a poll dependency, so a truncated list beats losing the list entirely. A
/// transport or parse failure on a page still fails the call — a silently
/// short list would be indistinguishable from a genuinely short one.
pub async fn list_calendars(
    transport: &impl CalendarListTransport,
    access_token: &str,
) -> Result<Vec<CalendarListEntry>, CalendarListError> {
    let mut calendars = Vec::new();
    let mut seen_page_tokens = std::collections::HashSet::new();
    let mut page_token: Option<String> = None;

    for _ in 0..MAX_PAGES {
        let body = transport
            .fetch_calendar_list_page(access_token, page_token.as_deref())
            .await
            .map_err(CalendarListError::Transport)?;

        let page: RawCalendarListPage = serde_json::from_str(&body)
            .map_err(|source| CalendarListError::InvalidResponse(source.to_string()))?;

        for item in page.items {
            // An entry with no id cannot be polled and cannot be toggled;
            // there is nothing to offer, so it is dropped rather than
            // failing the whole list.
            if let Some(id) = item.id {
                let summary = item.summary.unwrap_or_else(|| id.clone());
                calendars.push(CalendarListEntry { id, summary });
            }
        }

        match page.next_page_token {
            Some(token) if seen_page_tokens.insert(token.clone()) => page_token = Some(token),
            _ => break,
        }
    }

    Ok(calendars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Ordered scripted `(expected_page_token, response)` pairs. The expected
    /// token is asserted against what the caller actually passes, so a fake
    /// that ignores pagination fails the test instead of silently passing.
    type ScriptedPage = (Option<String>, Result<String, TransportError>);

    struct ScriptedListTransport {
        pages: Mutex<Vec<ScriptedPage>>,
        seen_tokens: Mutex<Vec<String>>,
    }

    impl ScriptedListTransport {
        fn new(pages: Vec<ScriptedPage>) -> Self {
            Self {
                pages: Mutex::new(pages),
                seen_tokens: Mutex::new(Vec::new()),
            }
        }
    }

    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    impl CalendarListTransport for ScriptedListTransport {
        async fn fetch_calendar_list_page(
            &self,
            access_token: &str,
            page_token: Option<&str>,
        ) -> Result<String, TransportError> {
            self.seen_tokens
                .lock()
                .unwrap()
                .push(access_token.to_string());
            let mut pages = self.pages.lock().unwrap();
            assert!(!pages.is_empty(), "scripted transport ran out of pages");
            let (expected_token, response) = pages.remove(0);
            assert_eq!(
                expected_token.as_deref(),
                page_token,
                "requested page_token {page_token:?}, expected {expected_token:?}"
            );
            response
        }
    }

    fn ok_page(body: &str) -> ScriptedPage {
        (None, Ok(body.to_string()))
    }

    #[tokio::test]
    async fn maps_id_and_summary_pairs_and_sends_the_token() {
        let transport = ScriptedListTransport::new(vec![ok_page(
            r#"{"items":[{"id":"primary","summary":"john@twinion.net"},
                {"id":"team@twinion.net","summary":"Team"}]}"#,
        )]);

        let calendars = list_calendars(&transport, "tok-1").await.unwrap();

        assert_eq!(
            calendars,
            vec![
                CalendarListEntry {
                    id: "primary".to_string(),
                    summary: "john@twinion.net".to_string(),
                },
                CalendarListEntry {
                    id: "team@twinion.net".to_string(),
                    summary: "Team".to_string(),
                },
            ]
        );
        assert_eq!(
            transport.seen_tokens.lock().unwrap().as_slice(),
            &["tok-1".to_string()]
        );
    }

    #[tokio::test]
    async fn a_missing_summary_falls_back_to_the_id() {
        let transport = ScriptedListTransport::new(vec![ok_page(r#"{"items":[{"id":"cal-1"}]}"#)]);

        let calendars = list_calendars(&transport, "tok-1").await.unwrap();

        assert_eq!(calendars[0].summary, "cal-1");
    }

    #[tokio::test]
    async fn items_with_no_id_are_dropped_and_a_missing_items_array_is_empty() {
        let transport = ScriptedListTransport::new(vec![
            ok_page(r#"{"items":[{"summary":"no id here"}]}"#),
            ok_page(r#"{}"#),
        ]);

        assert_eq!(list_calendars(&transport, "tok-1").await.unwrap(), vec![]);
        assert_eq!(list_calendars(&transport, "tok-1").await.unwrap(), vec![]);
    }

    #[tokio::test]
    async fn follows_next_page_token_so_calendars_past_the_first_page_are_selectable() {
        let transport = ScriptedListTransport::new(vec![
            (
                None,
                Ok(r#"{"items":[{"id":"cal-1"}],"nextPageToken":"page-2"}"#.to_string()),
            ),
            (
                Some("page-2".to_string()),
                Ok(r#"{"items":[{"id":"cal-2"}]}"#.to_string()),
            ),
        ]);

        let calendars = list_calendars(&transport, "tok-1").await.unwrap();

        assert_eq!(
            calendars.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["cal-1", "cal-2"]
        );
    }

    #[tokio::test]
    async fn a_repeated_page_token_stops_rather_than_looping_forever() {
        // Only two pages are scripted, so an implementation with no
        // repeated-token guard would panic on running out rather than
        // silently passing.
        let transport = ScriptedListTransport::new(vec![
            (
                None,
                Ok(r#"{"items":[{"id":"cal-1"}],"nextPageToken":"page-2"}"#.to_string()),
            ),
            (
                Some("page-2".to_string()),
                Ok(r#"{"items":[{"id":"cal-2"}],"nextPageToken":"page-2"}"#.to_string()),
            ),
        ]);

        let calendars = list_calendars(&transport, "tok-1").await.unwrap();

        assert_eq!(
            calendars.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["cal-1", "cal-2"]
        );
    }

    #[tokio::test]
    async fn a_401_is_reported_as_unauthorized_rather_than_an_empty_list() {
        let transport = ScriptedListTransport::new(vec![(
            None,
            Err(TransportError::http(401, "Google returned HTTP 401")),
        )]);

        let error = list_calendars(&transport, "stale").await.unwrap_err();

        assert!(error.is_unauthorized());
    }

    #[tokio::test]
    async fn an_unparseable_body_is_an_error_and_is_never_unauthorized() {
        let transport = ScriptedListTransport::new(vec![ok_page("not json")]);

        let error = list_calendars(&transport, "tok-1").await.unwrap_err();

        assert!(matches!(error, CalendarListError::InvalidResponse(_)));
        assert!(!error.is_unauthorized());
    }
}
