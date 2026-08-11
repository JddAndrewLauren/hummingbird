//! [`CalendarHostCore`]: the web host's one door into #72's `ContextPoller`
//! over #71's Google adapter (issue #73), kept free of `wasm_bindgen` so it
//! is testable with plain `cargo test` on any target — `lib.rs`'s
//! `wasm_bindings` module is the thin JS-facing shim over this.

use hummingbird_core::calendar::google::{
    CalendarListEntry, GoogleProviderPoller, ReqwestGoogleTransport,
};
use hummingbird_core::context::{ContextPoller, CredentialEvent, PollOutcome};

// The snapshot store: real IndexedDB in the browser, in-memory on any other
// target (only reached by `cargo test --workspace`, which never touches
// `wasm32`-gated code — see `client/core/src/storage/mod.rs`).
#[cfg(target_arch = "wasm32")]
type StoreImpl = hummingbird_core::storage::IndexedDbSnapshotStore;
#[cfg(not(target_arch = "wasm32"))]
type StoreImpl = hummingbird_core::storage::MemorySnapshotStore;

fn new_store(namespace: &str) -> StoreImpl {
    #[cfg(target_arch = "wasm32")]
    {
        StoreImpl::new(namespace)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = namespace;
        StoreImpl::default()
    }
}

type GooglePoller = ContextPoller<GoogleProviderPoller<ReqwestGoogleTransport>, StoreImpl>;

const SCHEMA_VERSION: u32 = 1;
const PROVIDER: &str = "google_calendar";

/// The response shape for `listCalendars` (issue #73's picker options).
///
/// A failure is reported as a `kind`, not thrown: the option list is a UX
/// nicety and never a poll dependency, so the host's job on a bad list is to
/// leave the picker as it stands, not to surface an error.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CalendarListResponse {
    /// `"ok"`, `"no_credential"` (nothing pushed yet, or polling is held on
    /// a rejected token), or `"failed"`.
    pub kind: &'static str,
    pub calendars: Vec<CalendarListEntry>,
}

/// Plain-Rust wrapper over one Google Calendar [`ContextPoller`], holding
/// exactly the operations the web host needs.
pub struct CalendarHostCore {
    poller: GooglePoller,
}

impl CalendarHostCore {
    pub fn new(namespace: String, calendar_ids: Vec<String>) -> Self {
        let store = new_store(&namespace);
        let transport = ReqwestGoogleTransport::default();
        let fetcher = GoogleProviderPoller::new(transport, calendar_ids);
        let poller = ContextPoller::new(PROVIDER, fetcher, store, SCHEMA_VERSION);
        Self { poller }
    }

    pub fn push_token(&mut self, token: String) {
        self.poller.push_token(token);
    }

    pub fn set_calendar_ids(&self, calendar_ids: Vec<String>) {
        self.poller.fetcher().set_calendar_ids(calendar_ids);
    }

    pub async fn start(&mut self, now_ms: i64) -> PollOutcome {
        self.poller.start(now_ms).await
    }

    pub async fn refresh(&mut self, now_ms: i64) -> PollOutcome {
        self.poller.refresh(now_ms).await
    }

    pub async fn on_timer(&mut self, now_ms: i64) -> PollOutcome {
        self.poller.on_timer(now_ms).await
    }

    /// The calendars this device's credential can read — the picker's
    /// options. Uses the token already pushed for polling rather than taking
    /// one: the host has no reason to hand the same credential across the
    /// boundary twice, and a token the core is holding on must not go out.
    pub async fn list_calendars(&self) -> CalendarListResponse {
        let Some(token) = self.poller.current_token() else {
            return CalendarListResponse {
                kind: "no_credential",
                calendars: Vec::new(),
            };
        };
        match self.poller.fetcher().list_calendars(&token).await {
            Ok(calendars) => CalendarListResponse {
                kind: "ok",
                calendars,
            },
            Err(_) => CalendarListResponse {
                kind: "failed",
                calendars: Vec::new(),
            },
        }
    }

    pub fn take_credential_events(&mut self) -> Vec<CredentialEvent> {
        self.poller.take_credential_events()
    }
}

/// Maps a [`PollOutcome`] to the stable string name the web host's protocol
/// (`client/web/src/store/protocol.ts`) matches on.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn outcome_name(outcome: PollOutcome) -> &'static str {
    match outcome {
        PollOutcome::NoCredential => "no_credential",
        PollOutcome::Held => "held",
        PollOutcome::Succeeded => "succeeded",
        PollOutcome::TransientFailure => "transient_failure",
        PollOutcome::Unauthorized => "unauthorized",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_fresh_host_has_no_credential_events() {
        let mut host = CalendarHostCore::new("test-ns".to_string(), vec!["primary".to_string()]);

        assert_eq!(host.take_credential_events(), Vec::new());
    }

    #[tokio::test]
    async fn start_with_no_pushed_token_reports_no_credential() {
        let mut host = CalendarHostCore::new("test-ns".to_string(), vec!["primary".to_string()]);
        let outcome = host.start(1_000).await;
        assert_eq!(outcome_name(outcome), "no_credential");
    }

    #[tokio::test]
    async fn listing_calendars_before_any_token_is_pushed_reports_no_credential() {
        // The one branch reachable without a network: it matters because the
        // picker calls this on a device whose silent re-mint failed, and a
        // "no_credential" answer must leave the existing options alone
        // rather than clearing them.
        let host = CalendarHostCore::new("test-ns".to_string(), vec!["primary".to_string()]);

        assert_eq!(
            host.list_calendars().await,
            CalendarListResponse {
                kind: "no_credential",
                calendars: Vec::new(),
            }
        );
    }

    #[tokio::test]
    async fn set_calendar_ids_is_readable_back_through_the_wrapped_fetcher() {
        let host = CalendarHostCore::new("test-ns".to_string(), vec!["a".to_string()]);
        host.set_calendar_ids(vec!["b".to_string(), "c".to_string()]);
        assert_eq!(
            host.poller.fetcher().calendar_ids(),
            vec!["b".to_string(), "c".to_string()]
        );
    }
}
