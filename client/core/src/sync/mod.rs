//! The owned-API sync engine (ADR-0008/0009): the read adapter (S2/#100) and
//! the write adapter (S3/#101) that sit either side of it.
//!
//! `adapter` is deserialisation plus validation over the shared
//! `hummingbird-domain` wire types — there is no mapping table, because
//! there is no foreign shape. `transport` is the HTTP seam (modelled on
//! `calendar::google::transport`) so the read adapter stays a pure function
//! of (token, responses), testable entirely against fixtures. `mirror` is
//! the device's local read model built from what the read adapter returns.
//! `write` (S3/#101) is the mirror-image seam on the write side: CAS
//! mutations, rebase-on-409, deterministic ids, and the error taxonomy the
//! outbound queue (#102) will drive. `queue` (S4/#102) is that outbound
//! queue: the durable FIFO structure and drain semantics built on top of
//! `write`.

pub mod adapter;
pub mod mirror;
pub mod queue;
pub mod reqwest_transport;
pub mod transport;
pub mod write;

pub use adapter::{fetch_delta, fetch_sweep, AdapterError};
pub use mirror::SyncMirror;
pub use queue::{DeadLetterEntry, DeadLetterReason, DrainOutcome, MutationIntent, OutboundQueue, QueueEntry};
pub use reqwest_transport::ReqwestSyncTransport;
pub use transport::{ChangesTransport, TransportError};
