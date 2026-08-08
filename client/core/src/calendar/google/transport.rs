//! The HTTP seam the adapter is injected with, so it stays a pure function
//! of (token, calendar ids, window, HTTP responses) — schedulable by #72
//! without change, and testable entirely against fixtures (no live
//! credentials, per #46's acceptance).
//!
//! Only a read method exists: fetching one page of
//! `calendars/{calendarId}/events`. There is no write-capable method on this
//! trait and never will be — the adapter only ever needs `calendar.readonly`
//! scope.

use std::fmt;

/// One page of `calendars/{calendarId}/events`, or the failure to fetch it.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait EventsTransport: Send + Sync {
    /// Fetch one page of events for `calendar_id` within `[time_min,
    /// time_max)`, both RFC 3339 timestamps. `page_token` is `None` for the
    /// first page and `Some` for every subsequent page, taken verbatim from
    /// the previous page's `nextPageToken`. Returns the raw JSON response
    /// body on success.
    async fn fetch_page(
        &self,
        calendar_id: &str,
        access_token: &str,
        time_min: &str,
        time_max: &str,
        page_token: Option<&str>,
    ) -> Result<String, TransportError>;
}

/// A failure to fetch or parse one page. Carries no partial data — the
/// adapter's complete-or-nothing guarantee depends on that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError {
    pub message: String,
    /// The HTTP status, when the failure was a response rather than a
    /// connection-level error.
    ///
    /// This exists for exactly one caller: #72's `ContextPoller` has to tell
    /// a dead credential (hold polling, ask the host for a new token) from a
    /// transient failure (retry on the next trigger), and a status buried in
    /// a `String` cannot carry that decision. Anything that needs to branch
    /// on *which* failure this was reads this field, never `message`.
    pub status: Option<u16>,
}

impl TransportError {
    /// A connection-level failure — no response arrived, so no status.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: None,
        }
    }

    /// A response arrived and was not a success.
    pub fn http(status: u16, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: Some(status),
        }
    }

    /// Whether Google rejected the credential itself (HTTP 401): the token
    /// is expired or revoked and no retry against it can succeed.
    pub fn is_unauthorized(&self) -> bool {
        self.status == Some(401)
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for TransportError {}
