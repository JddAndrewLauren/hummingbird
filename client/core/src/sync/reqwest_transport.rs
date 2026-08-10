//! [`ReqwestSyncTransport`]: the live [`ChangesTransport`] implementation
//! over `GET /api/changes?since=N` and `GET /api/sweep`, built on the
//! `reqwest::Client` the core already owns HTTP through (ADR-0003).
//!
//! No test here ever performs a live network call: the URL builder is the
//! only logic with a branch worth pinning, tested in isolation as a pure
//! function; the request-sending path is exercised end-to-end by the
//! adapter's fixture tests against a scripted transport.

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
}

impl ReqwestSyncTransport {
    /// `base_url` is the authority's origin (e.g.
    /// `https://hummingbird.example.workers.dev`), host-supplied per
    /// ADR-0003 — `core` invents no deployment addresses of its own.
    pub fn new(client: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self {
            client,
            base_url: base_url.into(),
        }
    }

    /// One authenticated GET returning the raw body, with the status
    /// preserved on failure — the adapter's unauthorized-vs-transient
    /// decision reads it.
    async fn get(&self, url: String, access_token: &str) -> Result<String, TransportError> {
        let response = self
            .client
            .get(url)
            .bearer_auth(access_token)
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
        self.get(build_changes_url(&self.base_url, since), access_token)
            .await
    }

    async fn fetch_sweep(&self, access_token: &str) -> Result<String, TransportError> {
        self.get(build_sweep_url(&self.base_url), access_token)
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
}
