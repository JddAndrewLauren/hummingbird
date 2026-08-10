//! [`ReqwestMutationTransport`]: the live [`MutationTransport`]
//! implementation, built on the `reqwest::Client` the core already owns
//! HTTP through (ADR-0003).
//!
//! No test here ever performs a live network call: the URL builder is the
//! only logic with a branch worth pinning, tested in isolation as a pure
//! function — the same split `sync::reqwest_transport` uses.

use super::transport::{
    HttpMethod, MutationRequest, MutationTransport, RawResponse, TransportError,
};

fn build_url(base_url: &str, path: &str) -> String {
    format!("{}{path}", base_url.trim_end_matches('/'))
}

#[derive(Debug, Clone)]
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
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl MutationTransport for ReqwestMutationTransport {
    async fn send(
        &self,
        access_token: &str,
        request: MutationRequest,
    ) -> Result<RawResponse, TransportError> {
        let url = build_url(&self.base_url, &request.path);
        let builder = match request.method {
            HttpMethod::Post => self.client.post(url),
            HttpMethod::Patch => self.client.patch(url),
            HttpMethod::Put => self.client.put(url),
        };

        let response = builder
            .bearer_auth(access_token)
            .header("content-type", "application/json")
            .body(request.body)
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
}
