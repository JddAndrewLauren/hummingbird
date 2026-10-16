//! [`ReqwestSyncTransport`]: the live [`ChangesTransport`] implementation
//! over `GET /api/changes?since=N` and `GET /api/sweep`, built on the
//! `reqwest::Client` the core already owns HTTP through (ADR-0003).
//!
//! No test here ever performs a live network call: the URL builder is the
//! only logic with a branch worth pinning, tested in isolation as a pure
//! function; the request-sending path is exercised end-to-end by the
//! adapter's fixture tests against a scripted transport — save the one
//! place a real socket is unavoidable, [`SYNC_REQUEST_TIMEOUT`]'s bound,
//! which `tests/sync_request_bound.rs` drives against a loopback listener
//! that accepts and never answers.

use std::time::Duration;

use crate::diagnostics::route::{
    CorrelationHeaders, HEADER_CLIENT_BUILD, HEADER_CLIENT_PLATFORM, HEADER_CYCLE_ID,
    HEADER_REQUEST_ID,
};

use super::transport::{ChangesTransport, TransportError};

/// The upper bound on one sync request's **total** time — connect, send,
/// headers and the whole response body — for every request the task core
/// makes in a cycle: the pull here and the queued-write drain in
/// [`super::write::ReqwestMutationTransport`] (#870).
///
/// **Why a bound at all.** A cycle holds the host's one task core for its
/// whole round trip (`MobileTaskHost`'s mutex, `TaskHostCore`'s checkout),
/// and capture, triage and every read need that same core. A request whose
/// connection is accepted but never answered would otherwise hold it until
/// the OS tears the socket down — minutes, on a phone.
///
/// **Why 15 seconds.** The calendar-token mint's `REQUEST_TIMEOUT` (on both
/// clients) is the precedent: this is "the authority is not answering",
/// not a UX-timed ceiling. It must stay under the web task worker's
/// `TASK_REQUEST_TIMEOUT_MS` (30s), so the browser's fetch is aborted and
/// the core checked back in *before* the serial queue would abandon the
/// wait and start answering `busy`.
///
/// **Why per request rather than on the client.** reqwest's wasm backend
/// has no client-level timeout; only `RequestBuilder::timeout` arms the
/// `AbortController` that actually cancels the browser's `fetch`. Set here,
/// in the shared transports, it bounds both hosts at once.
pub const SYNC_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// The `reqwest::Client` both hosts build for their two sync transports.
/// Native builds also bound the connect phase on the client itself, so a
/// request built on this client outside the two transports (the
/// [`super::write::DirectTransport`] calls share it) still cannot hang in
/// connect; the wasm backend has no such knob — the browser owns the socket.
pub fn sync_client() -> reqwest::Client {
    #[cfg(not(target_arch = "wasm32"))]
    {
        reqwest::Client::builder()
            .connect_timeout(SYNC_REQUEST_TIMEOUT)
            .build()
            .expect("a client with only a connect timeout always builds")
    }
    #[cfg(target_arch = "wasm32")]
    {
        reqwest::Client::new()
    }
}

/// The abort reason reqwest's wasm backend gives the `AbortController` when
/// a request's timeout fires. A timeout during `send()` comes back as
/// reqwest's own timeout error, but one that fires while the **body** is
/// being read surfaces only as this text inside a decode error's source.
const WASM_TIMEOUT_ABORT_REASON: &str = "reqwest::errors::TimedOut";

/// Whether a reqwest error's source chain says the request's own timeout
/// fired — on native, or on wasm in either phase (see
/// [`WASM_TIMEOUT_ABORT_REASON`]).
fn is_timeout(error: &reqwest::Error) -> bool {
    error.is_timeout() || names_the_wasm_timeout_abort(error)
}

/// Whether any cause under `error` carries [`WASM_TIMEOUT_ABORT_REASON`] —
/// split out over `dyn Error` so a native test can pin it, since no native
/// build can make the wasm backend's error.
fn names_the_wasm_timeout_abort(error: &dyn std::error::Error) -> bool {
    let mut source = error.source();
    while let Some(cause) = source {
        if cause.to_string().contains(WASM_TIMEOUT_ABORT_REASON) {
            return true;
        }
        source = cause.source();
    }
    false
}

/// A reqwest failure as a connection-level [`TransportError`]. reqwest's
/// `Display` names only the error's kind ("error sending request", "request
/// or response body error") and never its source, so a timeout would
/// otherwise reach `diagnostics::failure::classify_transport_error` as
/// `unknown` or `body`; it is named as one here instead, so the journal
/// records `timeout` on both hosts.
pub(crate) fn transport_error(error: reqwest::Error) -> TransportError {
    if is_timeout(&error) {
        TransportError::new(format!("request timed out: {error}"))
    } else {
        TransportError::new(error.to_string())
    }
}

/// Builds the `GET /api/changes?since=N` URL against `base_url` (the
/// authority's origin, host-supplied — `core` never hardcodes a deployment
/// address the way the Google transport hardcodes `googleapis.com`, because
/// this is the app's own server).
fn build_changes_url(base_url: &str, since: i64) -> String {
    format!(
        "{}/api/changes?since={since}",
        base_url.trim_end_matches('/')
    )
}

/// Builds the `GET /api/sweep` URL against `base_url`.
fn build_sweep_url(base_url: &str) -> String {
    format!("{}/api/sweep", base_url.trim_end_matches('/'))
}

#[derive(Debug, Clone)]
/// This transport holds **no client identity of its own** (#706). The
/// `X-Hummingbird-Client-Platform`/`-Build` values it attaches arrive per
/// call, on the [`CorrelationHeaders`] the caller passes in — minted and
/// sanitized in one place,
/// [`crate::diagnostics::DiagnosticsContext::correlation_headers`]. A
/// second, transport-held copy of the same identity was tried and removed
/// before landing: it was written by a setter and read by nothing on the
/// production path, which made it a decoy for the host slices (#709/#710)
/// that have to wire identity in. Wire it into the `DiagnosticsContext`.
pub struct ReqwestSyncTransport {
    client: reqwest::Client,
    base_url: String,
    timeout: Duration,
}

impl ReqwestSyncTransport {
    /// `base_url` is the authority's origin (e.g.
    /// `https://hummingbird.example.workers.dev`), host-supplied per
    /// ADR-0003 — `core` invents no deployment addresses of its own.
    pub fn new(client: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self {
            client,
            base_url: base_url.into(),
            timeout: SYNC_REQUEST_TIMEOUT,
        }
    }

    /// Replaces [`SYNC_REQUEST_TIMEOUT`] — for tests, which need a stalled
    /// request to fail in milliseconds rather than fifteen seconds.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Builds the request, attaching the four `X-Hummingbird-*` headers
    /// when `correlation` is present — split out from [`Self::get`] as a
    /// synchronous, no-network step so a test can call `.build()` on the
    /// result and inspect headers without ever sending anything.
    fn request(
        &self,
        url: String,
        access_token: &str,
        correlation: Option<&CorrelationHeaders<'_>>,
    ) -> reqwest::RequestBuilder {
        let mut builder = self
            .client
            .get(url)
            .bearer_auth(access_token)
            .timeout(self.timeout);
        if let Some(correlation) = correlation {
            builder = builder
                .header(HEADER_CYCLE_ID, correlation.cycle_id)
                .header(HEADER_REQUEST_ID, correlation.request_id)
                .header(HEADER_CLIENT_PLATFORM, correlation.platform)
                .header(HEADER_CLIENT_BUILD, correlation.build);
        }
        builder
    }

    /// One authenticated GET returning the raw body, with the status
    /// preserved on failure — the adapter's unauthorized-vs-transient
    /// decision reads it. `correlation`, when present, attaches the four
    /// `X-Hummingbird-*` headers the authority will later validate.
    async fn get(
        &self,
        url: String,
        access_token: &str,
        correlation: Option<&CorrelationHeaders<'_>>,
    ) -> Result<String, TransportError> {
        let response = self
            .request(url, access_token, correlation)
            .send()
            .await
            .map_err(transport_error)?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(TransportError::http(
                status.as_u16(),
                format!("the authority returned HTTP {status}"),
            ));
        }

        response.text().await.map_err(transport_error)
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl ChangesTransport for ReqwestSyncTransport {
    async fn fetch_changes(
        &self,
        access_token: &str,
        since: i64,
    ) -> Result<String, TransportError> {
        self.get(build_changes_url(&self.base_url, since), access_token, None)
            .await
    }

    async fn fetch_sweep(&self, access_token: &str) -> Result<String, TransportError> {
        self.get(build_sweep_url(&self.base_url), access_token, None)
            .await
    }

    async fn fetch_changes_with_headers(
        &self,
        access_token: &str,
        since: i64,
        headers: &CorrelationHeaders<'_>,
    ) -> Result<String, TransportError> {
        self.get(build_changes_url(&self.base_url, since), access_token, Some(headers))
            .await
    }

    async fn fetch_sweep_with_headers(
        &self,
        access_token: &str,
        headers: &CorrelationHeaders<'_>,
    ) -> Result<String, TransportError> {
        self.get(build_sweep_url(&self.base_url), access_token, Some(headers))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_changes_url_carries_the_since_cursor() {
        assert_eq!(
            build_changes_url("https://authority.example", 42),
            "https://authority.example/api/changes?since=42"
        );
    }

    #[test]
    fn the_changes_url_trims_a_trailing_slash_on_the_base() {
        assert_eq!(
            build_changes_url("https://authority.example/", 0),
            "https://authority.example/api/changes?since=0"
        );
    }

    #[test]
    fn the_sweep_url_carries_no_query_string() {
        assert_eq!(
            build_sweep_url("https://authority.example"),
            "https://authority.example/api/sweep"
        );
    }

    /// No headers are attached at all with `correlation: None` — the plain
    /// (non-observed) call path's exact wire shape, unchanged from before
    /// this issue.
    #[test]
    fn no_correlation_headers_are_attached_without_a_correlation() {
        let transport = ReqwestSyncTransport::new(reqwest::Client::new(), "https://authority.example");
        let request = transport
            .request("https://authority.example/api/sweep".to_string(), "token", None)
            .build()
            .unwrap();
        assert!(request.headers().get(HEADER_CYCLE_ID).is_none());
        assert!(request.headers().get(HEADER_REQUEST_ID).is_none());
    }

    /// The bound rides on every request this transport builds — the
    /// per-request timeout is the only one reqwest's wasm backend honours.
    #[test]
    fn every_request_carries_the_sync_bound() {
        let transport = ReqwestSyncTransport::new(reqwest::Client::new(), "https://authority.example");
        let request = transport
            .request("https://authority.example/api/sweep".to_string(), "token", None)
            .build()
            .unwrap();
        assert_eq!(request.timeout(), Some(&SYNC_REQUEST_TIMEOUT));
    }

    /// The bound is under the web task worker's 30s abandon bound
    /// (`TASK_REQUEST_TIMEOUT_MS`), so the fetch is cancelled first.
    #[test]
    fn the_sync_bound_is_under_the_web_workers_abandon_bound() {
        assert!(SYNC_REQUEST_TIMEOUT < Duration::from_millis(30_000));
    }

    /// What reqwest's wasm backend hands back when the bound fires while the
    /// body is being read: a decode error whose source is the abort reason,
    /// formatted as a `JsValue`'s debug text (reqwest's `error::wasm`).
    #[derive(Debug)]
    struct WasmBodyAbort(JsAbortReason);

    #[derive(Debug)]
    struct JsAbortReason;

    impl std::fmt::Display for WasmBodyAbort {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("error decoding response body")
        }
    }

    impl std::error::Error for WasmBodyAbort {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    impl std::fmt::Display for JsAbortReason {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("JsValue(\"reqwest::errors::TimedOut\")")
        }
    }

    impl std::error::Error for JsAbortReason {}

    #[test]
    fn a_wasm_body_read_aborted_by_the_bound_reads_as_a_timeout() {
        assert!(names_the_wasm_timeout_abort(&WasmBodyAbort(JsAbortReason)));
    }

    #[test]
    fn an_error_whose_causes_never_name_the_abort_is_not_a_timeout() {
        assert!(!names_the_wasm_timeout_abort(&JsAbortReason));
    }

    /// Every one of the four `X-Hummingbird-*` headers lands on the request
    /// exactly as given — built, never sent, so this needs no network.
    #[test]
    fn all_four_correlation_headers_are_attached_when_present() {
        let transport = ReqwestSyncTransport::new(reqwest::Client::new(), "https://authority.example");
        let correlation = CorrelationHeaders {
            cycle_id: "cycle-1",
            request_id: "cycle-1-0",
            platform: "web",
            build: "1.2.3",
        };
        let request = transport
            .request(
                "https://authority.example/api/sweep".to_string(),
                "token",
                Some(&correlation),
            )
            .build()
            .unwrap();
        assert_eq!(request.headers().get(HEADER_CYCLE_ID).unwrap(), "cycle-1");
        assert_eq!(request.headers().get(HEADER_REQUEST_ID).unwrap(), "cycle-1-0");
        assert_eq!(request.headers().get(HEADER_CLIENT_PLATFORM).unwrap(), "web");
        assert_eq!(request.headers().get(HEADER_CLIENT_BUILD).unwrap(), "1.2.3");
    }
}
