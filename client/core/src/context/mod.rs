//! The context-provider credential seam and poll lifecycle (issue #72, per
//! ADR-0005): a host-pushed, in-memory-only token per provider, and the
//! `start`/`refresh`/timer trigger points that drive a provider's fetch
//! (#71's Google adapter, a future M365 one) through #68's atomic snapshot
//! store.
//!
//! Nothing here persists a credential — [`storage::Persistable`] is the only
//! door into the snapshot store, and no credential type ever implements it.
//! Expiry or a 401 holds a provider's polling and surfaces one
//! [`CredentialEvent`]; a fresh [`ContextPoller::push_token`] is the only way
//! out.
//!
//! [`storage::Persistable`]: crate::storage::Persistable

mod credential;
mod poller;

pub use poller::{ContextPoller, CredentialEvent, PollFailure, PollOutcome, ProviderPoller};
