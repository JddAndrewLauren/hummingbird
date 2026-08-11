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
//! active-issue-count machinery around it. It persists each of its two
//! slots **only when that slot's value actually changed** (#165), which is
//! what keeps a 60-second cadence from rewriting the whole read model
//! against a mirror nothing touched; see [`cycle`]'s own docs for why
//! skipping is safe and what it costs the envelope's `as_of`.
//!
//! **The seed-minting rule (#223).** Every `Core` mutation entry point
//! takes a caller-minted `seed` and feeds it to [`write::deterministic_id`]
//! — caller-minted rather than sampled here, because bare
//! `wasm32-unknown-unknown` has no clock or RNG that does not panic. Which
//! *shape* of seed a caller mints is decided by one criterion, not a list
//! of today's call sites: **which id the seed's hash becomes**. For a
//! mutation that touches an entity that **already exists**, the hash is
//! only the mutation's own `QueueEntry` id — a local durable-queue key
//! that never crosses the wire, since the intent is a CAS write against
//! the entity's existing id — so the seed is **deterministic**, composed
//! from that entity's identity, the operation, and the caller's `now_ms`:
//! a retried or crash-replayed enqueue of the identical intent (same
//! identity, same operation, same instant) reproduces the identical entry
//! *id* rather than minting a second, unrelated one, and the dead-letter
//! journal can name the entry it buried. Identity only:
//! [`queue::OutboundQueue::enqueue`] is a bare append with no id dedupe, so
//! two such enqueues are two entries sharing one id, never one entry — a
//! determinism this criterion uses for naming, not for collapsing
//! duplicates. For a mutation that mints a **new** entity, the
//! hash *is* the entity's id, landing on the authority's client-id-keyed
//! create path — so the seed must be **non-deterministic**, because two
//! identical intents in the same millisecond must become two entities,
//! never collide into one. The create-side idempotency lives there and
//! only there: `Core::capture`'s offline-replay dedup comes from a retry
//! reusing that capture's own already-minted seed (the queue holds it, so
//! the replayed create carries the identical id and lands on the
//! authority's "already exists" path), never from the minting function's
//! output being predictable. A future mutation kind is tested against
//! this same criterion — which id does its seed become? — not appended to
//! an enumeration; the web shell's seed-minting functions reference this
//! paragraph rather than restating it.

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
