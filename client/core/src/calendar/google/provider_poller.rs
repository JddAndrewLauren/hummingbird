//! [`GoogleProviderPoller`]: the [`crate::context::ProviderPoller`] impl that
//! plugs #71's [`fetch_calendar_snapshot`] into #72's [`ContextPoller`]
//! (issue #73).
//!
//! `ContextPoller::attempt` calls `poll(&self, access_token, now_ms)` with no
//! slot for the selected calendar ids (review note from #84), so this type
//! carries them itself, behind a `RwLock` so the host can update the
//! selection (the calendar picker) between poll attempts without needing a
//! fresh `ContextPoller`.
//!
//! [`crate::context::ProviderPoller::poll`]'s only failure classification is
//! [`crate::context::PollFailure::Unauthorized`] vs `Transient`. #72's review
//! flagged that [`TransportError`] carries no HTTP status yet, so every
//! [`AdapterError`] here maps to `Transient` — wiring a real 401 through to
//! `Unauthorized` needs a status-carrying transport, tracked separately.
//!
//! [`ContextPoller`]: crate::context::ContextPoller

use std::sync::RwLock;

use crate::context::{PollFailure, ProviderPoller};

use super::adapter::fetch_calendar_snapshot;
use super::transport::EventsTransport;
use crate::calendar::CalendarSnapshot;

/// Wraps an [`EventsTransport`] with the host-supplied, host-mutable list of
/// selected calendar ids the picker (#73) drives.
pub struct GoogleProviderPoller<T: EventsTransport> {
    transport: T,
    calendar_ids: RwLock<Vec<String>>,
}

impl<T: EventsTransport> GoogleProviderPoller<T> {
    pub fn new(transport: T, calendar_ids: Vec<String>) -> Self {
        Self {
            transport,
            calendar_ids: RwLock::new(calendar_ids),
        }
    }

    /// Replaces the selected calendar ids. Takes effect on the next poll
    /// attempt; a poll already in flight keeps the ids it started with.
    pub fn set_calendar_ids(&self, calendar_ids: Vec<String>) {
        *self.calendar_ids.write().expect("calendar_ids lock poisoned") = calendar_ids;
    }

    pub fn calendar_ids(&self) -> Vec<String> {
        self.calendar_ids
            .read()
            .expect("calendar_ids lock poisoned")
            .clone()
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<T: EventsTransport + Send + Sync> ProviderPoller for GoogleProviderPoller<T> {
    type Snapshot = CalendarSnapshot;

    async fn poll(&self, access_token: &str, now_ms: i64) -> Result<CalendarSnapshot, PollFailure> {
        let calendar_ids = self.calendar_ids();
        fetch_calendar_snapshot(&self.transport, access_token, &calendar_ids, now_ms)
            .await
            .map_err(|source| PollFailure::Transient(source.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::google::TransportError;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct ScriptedTransport {
        // calendar_id -> raw JSON body for its single page
        pages: Mutex<HashMap<String, String>>,
        seen_calendar_ids: Mutex<Vec<String>>,
    }

    impl ScriptedTransport {
        fn new(pages: HashMap<String, String>) -> Self {
            Self {
                pages: Mutex::new(pages),
                seen_calendar_ids: Mutex::new(Vec::new()),
            }
        }
    }

    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    impl EventsTransport for ScriptedTransport {
        async fn fetch_page(
            &self,
            calendar_id: &str,
            _access_token: &str,
            _time_min: &str,
            _time_max: &str,
            _page_token: Option<&str>,
        ) -> Result<String, TransportError> {
            self.seen_calendar_ids
                .lock()
                .unwrap()
                .push(calendar_id.to_string());
            self.pages
                .lock()
                .unwrap()
                .get(calendar_id)
                .cloned()
                .ok_or_else(|| TransportError::new(format!("no page for {calendar_id}")))
        }
    }

    fn empty_page() -> String {
        r#"{"items":[]}"#.to_string()
    }

    #[tokio::test]
    async fn poll_queries_exactly_the_currently_selected_calendars() {
        let mut pages = HashMap::new();
        pages.insert("cal-a".to_string(), empty_page());
        pages.insert("cal-b".to_string(), empty_page());
        let transport = ScriptedTransport::new(pages);
        let poller = GoogleProviderPoller::new(transport, vec!["cal-a".to_string()]);

        let result = poller.poll("token", 1_000).await;

        assert!(result.is_ok());
        assert_eq!(
            poller.transport.seen_calendar_ids.lock().unwrap().as_slice(),
            &["cal-a".to_string()]
        );
    }

    #[tokio::test]
    async fn set_calendar_ids_changes_what_the_next_poll_queries() {
        let mut pages = HashMap::new();
        pages.insert("cal-a".to_string(), empty_page());
        pages.insert("cal-b".to_string(), empty_page());
        let transport = ScriptedTransport::new(pages);
        let poller = GoogleProviderPoller::new(transport, vec!["cal-a".to_string()]);

        poller.set_calendar_ids(vec!["cal-b".to_string()]);
        let result = poller.poll("token", 1_000).await;

        assert!(result.is_ok());
        assert_eq!(
            poller.transport.seen_calendar_ids.lock().unwrap().as_slice(),
            &["cal-b".to_string()]
        );
    }

    #[tokio::test]
    async fn an_adapter_error_maps_to_a_transient_poll_failure() {
        // No scripted page for "cal-missing": the transport returns a
        // TransportError, which the adapter wraps as AdapterError::Transport.
        let transport = ScriptedTransport::new(HashMap::new());
        let poller = GoogleProviderPoller::new(transport, vec!["cal-missing".to_string()]);

        let result = poller.poll("token", 1_000).await;

        assert!(matches!(result, Err(PollFailure::Transient(_))));
    }
}
