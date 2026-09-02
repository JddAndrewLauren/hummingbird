//! [`ReqwestMutationTransport`]: the live [`MutationTransport`]
//! implementation, built on the `reqwest::Client` the core already owns
//! HTTP through (ADR-0003).
//!
//! No test here ever performs a live network call: the URL builder is the
//! only logic with a branch worth pinning, tested in isolation as a pure
//! function — the same split `sync::reqwest_transport` uses. The four
//! `X-Hummingbird-*` correlation headers (#706) are the one exception that
//! needs a built-but-unsent `reqwest::Request` to assert against — see
//! [`ReqwestMutationTransport::request`].

use crate::diagnostics::route::{
    CorrelationHeaders, HEADER_CLIENT_BUILD, HEADER_CLIENT_PLATFORM, HEADER_CYCLE_ID,
    HEADER_REQUEST_ID,
};

use super::transport::{
    HttpMethod, MutationRequest, MutationTransport, RawResponse, TransportError,
};

fn build_url(base_url: &str, path: &str) -> String {
    format!("{}{path}", base_url.trim_end_matches('/'))
}

#[derive(Debug, Clone)]
/// Holds no client identity of its own — see
/// `sync::reqwest_transport::ReqwestSyncTransport`'s docs for why the
/// `X-Hummingbird-Client-*` values arrive per call on
/// [`CorrelationHeaders`] instead.
pub struct ReqwestMutationTransport {
    client: reqwest::Client,
    base_url: String,
}

impl ReqwestMutationTransport {
    /// `base_url` is the authority's origin, host-supplied per ADR-0003.
    pub fn new(client: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self {
            client,
            base_url: base_url.into(),
        }
    }

    /// Builds the request, attaching the four `X-Hummingbird-*` headers
    /// when `correlation` is present — a synchronous, no-network step so a
    /// test can call `.build()` on the result and inspect headers without
    /// sending anything.
    fn request(
        &self,
        request: &MutationRequest,
        access_token: &str,
        correlation: Option<&CorrelationHeaders<'_>>,
    ) -> reqwest::RequestBuilder {
        let url = build_url(&self.base_url, &request.path);
        let builder = match request.method {
            HttpMethod::Post => self.client.post(url),
            HttpMethod::Patch => self.client.patch(url),
            HttpMethod::Put => self.client.put(url),
        };
        let mut builder = builder
            .bearer_auth(access_token)
            .header("content-type", "application/json");
        if let Some(correlation) = correlation {
            builder = builder
                .header(HEADER_CYCLE_ID, correlation.cycle_id)
                .header(HEADER_REQUEST_ID, correlation.request_id)
                .header(HEADER_CLIENT_PLATFORM, correlation.platform)
                .header(HEADER_CLIENT_BUILD, correlation.build);
        }
        builder
    }

    async fn send_inner(
        &self,
        access_token: &str,
        request: MutationRequest,
        correlation: Option<&CorrelationHeaders<'_>>,
    ) -> Result<RawResponse, TransportError> {
        let body = request.body.clone();
        let response = self
            .request(&request, access_token, correlation)
            .body(body)
            .send()
            .await
            .map_err(|source| TransportError::new(source.to_string()))?;

        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|source| TransportError::new(source.to_string()))?;
        Ok(RawResponse { status, body })
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl MutationTransport for ReqwestMutationTransport {
    async fn send(
        &self,
        access_token: &str,
        request: MutationRequest,
    ) -> Result<RawResponse, TransportError> {
        self.send_inner(access_token, request, None).await
    }

    async fn send_with_headers(
        &self,
        access_token: &str,
        request: MutationRequest,
        headers: &CorrelationHeaders<'_>,
    ) -> Result<RawResponse, TransportError> {
        self.send_inner(access_token, request, Some(headers)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_url_joins_the_base_and_the_path() {
        assert_eq!(
            build_url("https://authority.example", "/api/items"),
            "https://authority.example/api/items"
        );
    }

    #[test]
    fn the_url_trims_a_trailing_slash_on_the_base() {
        assert_eq!(
            build_url("https://authority.example/", "/api/items/a-1"),
            "https://authority.example/api/items/a-1"
        );
    }

    fn create_request() -> MutationRequest {
        MutationRequest {
            method: HttpMethod::Post,
            path: "/api/items".to_string(),
            body: "{}".to_string(),
            operation_id: None,
        }
    }

    #[test]
    fn no_correlation_headers_are_attached_without_a_correlation() {
        let transport = ReqwestMutationTransport::new(reqwest::Client::new(), "https://authority.example");
        let request = transport
            .request(&create_request(), "token", None)
            .build()
            .unwrap();
        assert!(request.headers().get(HEADER_CYCLE_ID).is_none());
    }

    #[test]
    fn all_four_correlation_headers_are_attached_when_present() {
        let transport =
            ReqwestMutationTransport::new(reqwest::Client::new(), "https://authority.example");
        let correlation = CorrelationHeaders {
            cycle_id: "cycle-9",
            request_id: "cycle-9-3",
            platform: "android",
            build: "42",
        };
        let request = transport
            .request(&create_request(), "token", Some(&correlation))
            .build()
            .unwrap();
        assert_eq!(request.headers().get(HEADER_CYCLE_ID).unwrap(), "cycle-9");
        assert_eq!(request.headers().get(HEADER_REQUEST_ID).unwrap(), "cycle-9-3");
        assert_eq!(request.headers().get(HEADER_CLIENT_PLATFORM).unwrap(), "android");
        assert_eq!(request.headers().get(HEADER_CLIENT_BUILD).unwrap(), "42");
    }
}
