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
//! `write`. `cycle` (S5/#103) is ADR-0007/ADR-0008's one cycle — drain then
//! pull, delta as the normal pull with a full sweep as the backstop —
//! wired on top of `queue`, `adapter`, and `mirror`, plus the backoff and
//! active-issue-count machinery around it.
//!
//! **The seed-minting rule (#223).** Every `Core` mutation entry point
//! takes a caller-minted `seed` and feeds it to [`write::deterministic_id`]
//! — caller-minted rather than sampled here, because bare
//! `wasm32-unknown-unknown` has no clock or RNG that does not panic. Which
//! *shape* of seed a caller mints is decided by one criterion, not a list
//! of today's call sites: a mutation naming an entity that **already
//! exists** mints a **deterministic** seed, composed from that entity's own
//! identity plus the caller's `now_ms`, so a retry of the identical intent
//! (same identity, same operation, same instant) reproduces the identical
//! seed and therefore the identical id — idempotent against the
//! authority's client-id-keyed create path (`Core::act`, `Core::triage`,
//! `Core::set_binding`). A mutation minting a **new** entity has no prior
//! identity to derive a seed from and must mint a **non-deterministic**
//! seed instead, because two identical intents in the same millisecond
//! would otherwise collide into a single entity (`Core::capture`, the one
//! entry point that creates rather than touches). A future mutation kind
//! is tested against this same criterion, not appended to an enumeration;
//! the seed-minting call sites in the web shell (`useCaptureWiring.ts`,
//! `useItemActions.ts`, `useTriageWiring.ts`, `useBindingsWiring.ts`)
//! reference this paragraph rather than restating it.

pub mod adapter;
pub mod cycle;
pub mod mirror;
pub mod queue;
pub mod reqwest_transport;
pub mod transport;
pub mod write;

pub use adapter::{fetch_delta, fetch_sweep, AdapterError};
pub use cycle::{Backoff, CycleOutcome, LoadError, SyncCycle, Trigger};
pub use mirror::SyncMirror;
pub use queue::{DeadLetterEntry, DeadLetterReason, DrainOutcome, MutationIntent, OutboundQueue, QueueEntry};
pub use reqwest_transport::ReqwestSyncTransport;
pub use transport::{ChangesTransport, TransportError};
