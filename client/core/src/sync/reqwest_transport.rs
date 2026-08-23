//! [`ReqwestSyncTransport`]: the live [`ChangesTransport`] implementation
//! over `GET /api/changes?since=N` and `GET /api/sweep`, built on the
//! `reqwest::Client` the core already owns HTTP through (ADR-0003).
//!
//! No test here ever performs a live network call: the URL builder is the
//! only logic with a branch worth pinning, tested in isolation as a pure
//! function; the request-sending path is exercised end-to-end by the
//! adapter's fixture tests against a scripted transport.

use crate::diagnostics::route::{
    CorrelationHeaders, HEADER_CLIENT_BUILD, HEADER_CLIENT_PLATFORM, HEADER_CYCLE_ID,
    HEADER_REQUEST_ID,
};

use super::transport::{ChangesTransport, TransportError};

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
pub struct ReqwestSyncTransport {
    client: reqwest::Client,
    base_url: String,
    /// Host identity for the `X-Hummingbird-Client-*` headers (#706) — set
    /// via [`ReqwestSyncTransport::with_client_identity`]. Defaults to
    /// `"unknown"` (which still satisfies the header-value pattern) rather
    /// than requiring every existing caller of [`ReqwestSyncTransport::new`]
    /// to supply one.
    platform: String,
    build: String,
}

impl ReqwestSyncTransport {
    /// `base_url` is the authority's origin (e.g.
    /// `https://hummingbird.example.workers.dev`), host-supplied per
    /// ADR-0003 — `core` invents no deployment addresses of its own.
    pub fn new(client: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self {
            client,
            base_url: base_url.into(),
            platform: "unknown".to_string(),
            build: "unknown".to_string(),
        }
    }

    /// Sets the `X-Hummingbird-Client-Platform`/`-Build` header values a
    /// correlated call attaches. Additive and optional — see the struct
    /// docs on why `new` alone still compiles and behaves.
    pub fn with_client_identity(mut self, platform: impl Into<String>, build: impl Into<String>) -> Self {
        self.platform = platform.into();
        self.build = build.into();
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
        let mut builder = self.client.get(url).bearer_auth(access_token);
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
            .map_err(|source| TransportError::new(source.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(TransportError::http(
                status.as_u16(),
                format!("the authority returned HTTP {status}"),
            ));
        }

        response
            .text()
            .await
            .map_err(|source| TransportError::new(source.to_string()))
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

    /// Every one of the four `X-Hummingbird-*` headers lands on the request
    /// exactly as given — built, never sent, so this needs no network.
    #[test]
    fn all_four_correlation_headers_are_attached_when_present() {
        let transport = ReqwestSyncTransport::new(reqwest::Client::new(), "https://authority.example")
            .with_client_identity("web", "1.2.3");
        let correlation = CorrelationHeaders {
            cycle_id: "cycle-1",
            request_id: "cycle-1-0",
            platform: &transport.platform.clone(),
            build: &transport.build.clone(),
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
