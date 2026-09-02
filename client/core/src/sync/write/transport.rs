//! The HTTP seam the write adapter is injected with (#101), modelled on
//! [`super::super::transport::ChangesTransport`] but shaped differently on
//! purpose: a read either succeeds or fails outright, so that trait
//! collapses every non-2xx into an `Err`. A write's non-2xx bodies are
//! signal — 409 carries the current entity the adapter rebases onto, and
//! the taxonomy needs the status to classify the rest — so this trait
//! returns the whole response and only fails on a connection-level error
//! (no response arrived at all).
//!
//! That connection-level failure is genuinely the same concept the read
//! transport already carries, so it reuses
//! [`TransportError`](super::super::transport::TransportError) rather than
//! adding a third copy alongside `calendar::google::transport`'s — only via
//! [`TransportError::new`], since a write transport never needs the
//! status-carrying `.http()` constructor (status lives on [`RawResponse`]
//! instead).

use crate::diagnostics::route::CorrelationHeaders;
pub use super::super::transport::TransportError;

/// The HTTP verb a mutation is sent with. `PATCH` and `PUT` are both CAS
/// writes (`expected_version` plus absolute sets); `POST` is a
/// idempotent-by-id create.
///
/// `Serialize`/`Deserialize` so the outbound queue (#102) can persist a
/// queued mutation's method as part of its durable entry, rather than
/// re-deriving it from the path at drain time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HttpMethod {
    Post,
    Patch,
    Put,
}

/// One CAS write or create, transport-agnostic: a method, a path (e.g.
/// `/api/items` or `/api/items/uuid-1`), and the serialized JSON body.
#[derive(Debug, Clone, PartialEq)]
pub struct MutationRequest {
    pub method: HttpMethod,
    pub path: String,
    pub body: String,
    /// The id of the operation that requested this write (#739), if any —
    /// carried from the queued [`super::super::queue::QueueEntry`] that
    /// produced this request so the instrumented transport can stamp its
    /// `http.started`/`http.finished` with the same id
    /// `operation.local_commit` carried, joining the two across the
    /// outbound-queue boundary.
    pub operation_id: Option<String>,
}

/// A completed round trip, whatever its status. Unlike the read transport,
/// a write's failure responses (400, 401, 409, 429, 5xx) are not
/// `TransportError` — they are ordinary [`RawResponse`]s the taxonomy
/// classifies, because their bodies (the 409's current entity, the 4xx's
/// message) are meaningful to the caller.
#[derive(Debug, Clone, PartialEq)]
pub struct RawResponse {
    pub status: u16,
    pub body: String,
}

/// One call to the mutation endpoint, or the failure to make it at all.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait MutationTransport: Send + Sync {
    async fn send(
        &self,
        access_token: &str,
        request: MutationRequest,
    ) -> Result<RawResponse, TransportError>;

    /// The same call as [`MutationTransport::send`], plus the four
    /// `X-Hummingbird-*` correlation headers (#706) an observed sync cycle
    /// attaches. Defaults to ignoring `headers` and delegating to
    /// [`MutationTransport::send`] — see
    /// [`super::super::transport::ChangesTransport::fetch_changes_with_headers`]
    /// for why every existing implementation keeps compiling unchanged.
    async fn send_with_headers(
        &self,
        access_token: &str,
        request: MutationRequest,
        headers: &CorrelationHeaders<'_>,
    ) -> Result<RawResponse, TransportError> {
        let _ = headers;
        self.send(access_token, request).await
    }
}
