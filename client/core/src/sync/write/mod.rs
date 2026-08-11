//! The owned-API write adapter (ADR-0008/0009, S3/#101): CAS mutations,
//! rebase-on-409, deterministic ids, and the error taxonomy the outbound
//! queue (#102) depends on.
//!
//! `id` mints deterministic ids for locally-created entities. `transport`
//! is the HTTP seam for mutations (modelled on `sync::transport`, reusing
//! its [`TransportError`](super::transport::TransportError) for
//! connection-level failures rather than adding a third copy). `taxonomy`
//! classifies every non-2xx response. `rebase` is the pure field-diff that
//! decides whether a 409 auto-resolves. `adapter` ties them together into
//! `create`/`patch_with_rebase`, generic over any of ADR-0009's entity and
//! patch DTOs (items, steps, blocked_by, projects, routes, fog, settings) —
//! there is no per-entity copy, because the CAS contract (absolute sets,
//! `expected_version`, 409-carries-current) is identical across all of
//! them.

pub mod adapter;
pub mod id;
pub mod paths;
pub mod rebase;
#[cfg(feature = "reqwest-transport")]
pub mod reqwest_transport;
pub mod taxonomy;
pub mod transport;

pub use adapter::{create, patch_with_rebase, CreateOutcome};
pub use id::deterministic_id;
pub use rebase::RebaseDecision;
#[cfg(feature = "reqwest-transport")]
pub use reqwest_transport::ReqwestMutationTransport;
pub use taxonomy::WriteError;
pub use transport::{HttpMethod, MutationRequest, MutationTransport, RawResponse};
