//! The HTTP seam the sync adapter is injected with (modelled on
//! `calendar::google::transport`), so the adapter stays a pure function of
//! (token, since, responses) — testable entirely against fixtures, no live
//! credentials and no network (this issue's acceptance).
//!
//! Only read methods exist: one call for `GET /api/changes?since=N` and one
//! for `GET /api/sweep`. There is no write-capable method on this trait —
//! S3/#101 injects a separate trait for CAS mutations.

use std::fmt;

/// One call to either read endpoint, or the failure to make it.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait ChangesTransport: Send + Sync {
    /// `GET /api/changes?since={since}`. Returns the raw JSON response body
    /// on success.
    async fn fetch_changes(&self, access_token: &str, since: i64)
        -> Result<String, TransportError>;

    /// `GET /api/sweep`. Returns the raw JSON response body on success.
    async fn fetch_sweep(&self, access_token: &str) -> Result<String, TransportError>;
}

/// A failure to fetch or parse one response. Carries no partial data — the
/// adapter's complete-or-nothing guarantee depends on that: either a whole
/// [`hummingbird_domain::ChangesResponse`] comes back, or nothing does.
///
/// Deliberately its own type rather than a shared import of
/// `calendar::google::transport::TransportError`, even though the two are
/// currently field-for-field identical: they are different lanes (a
/// third-party read-only API vs. this app's own read/write authority) with
/// no reason to evolve in lockstep — a status-carrying variant a future
/// Google quirk needs has no business showing up in every `sync` transport
/// error. #101 is about to add a CAS-write transport error alongside this
/// one; if that one also turns out identical, that is the point to decide
/// on a shared base type instead of three copies, not before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError {
    pub message: String,
    /// The HTTP status, when the failure was a response rather than a
    /// connection-level error. Load-bearing for exactly one branch: telling
    /// a dead credential (401 — hold, ask the host for a new token) from a
    /// transient failure (retry on the next trigger).
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

    /// Whether the authority rejected the credential itself (HTTP 401): the
    /// token is missing, unknown, or revoked, and no retry against it can
    /// succeed. Distinct from 403 (wrong scope) — a `device`-scoped read
    /// call never sees 403, but a transient failure must never be confused
    /// with this either.
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
