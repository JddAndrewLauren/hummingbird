//! The owned-API read adapter (ADR-0008/0009, S2/#100): two reads, one
//! injected transport, and the reconciling mirror they feed.
//!
//! `adapter` is deserialisation plus validation over the shared
//! `hummingbird-domain` wire types — there is no mapping table, because
//! there is no foreign shape. `transport` is the HTTP seam (modelled on
//! `calendar::google::transport`) so the adapter stays a pure function of
//! (token, responses), testable entirely against fixtures. `mirror` is the
//! device's local read model built from what the adapter returns.

pub mod adapter;
pub mod mirror;
pub mod reqwest_transport;
pub mod transport;

pub use adapter::{fetch_delta, fetch_sweep, AdapterError};
pub use mirror::SyncMirror;
pub use reqwest_transport::ReqwestSyncTransport;
pub use transport::{ChangesTransport, TransportError};
