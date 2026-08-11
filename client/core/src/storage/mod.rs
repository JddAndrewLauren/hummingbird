//! Core-owned persistence (ADR-0003): an internal snapshot store trait with
//! a compile-target split — in-memory (the cross-platform test default),
//! `std::fs` write-temp-then-rename (non-`wasm32`), and IndexedDB
//! (`wasm32`).
//!
//! The trait is internal to `core` — there is no host-implemented storage
//! callback (ADR-0003's rejected alternative). The host contributes exactly
//! one thing at init: a storage path/namespace, handed to whichever
//! implementation the target picks. Writes are atomic: a snapshot write
//! either fully replaces the previous snapshot or leaves it intact, which is
//! what ADR-0007 and issue #46 lean on.

mod envelope;
mod memory;

#[cfg(not(target_arch = "wasm32"))]
mod fs;

#[cfg(target_arch = "wasm32")]
mod indexed_db;

pub use envelope::{Envelope, Persistable};
/// `Persistable`'s sealing supertrait, nameable inside `core` only — see
/// [`Persistable`]. Every `impl Persistable` in this crate needs a matching
/// `impl PersistableSealed`.
pub(crate) use envelope::sealed::PersistableSealed;
pub use memory::MemorySnapshotStore;
// The fallible, counting `MemorySnapshotStore` twin — test-only, and
// necessarily `core`-internal: `sealed::Sealed` is unnameable outside this
// crate, so no integration test or benchmark could implement one.
#[cfg(test)]
pub(crate) use memory::InstrumentedSnapshotStore;

#[cfg(not(target_arch = "wasm32"))]
pub use fs::FsSnapshotStore;

#[cfg(target_arch = "wasm32")]
pub use indexed_db::{IndexedDbError, IndexedDbSnapshotStore};

/// Sealing boundary for [`SnapshotStore`] (the Rust API guidelines'
/// sealed-trait pattern, C-SEALED): `sealed` is a private module, so
/// `sealed::Sealed` — despite being `pub trait` — cannot be named or
/// implemented from outside this crate. That is what keeps `write`/`read`
/// off the public API surface: an external crate can neither call them
/// (method resolution requires the trait in scope, and `Sealed` cannot be
/// imported) nor implement `SnapshotStore` for its own type (the blanket
/// impl below requires `Sealed`, which it also cannot implement). Only
/// [`save_snapshot`]/[`load_snapshot`]/[`load_snapshot_at_version`] — each
/// gated on [`Persistable`] — reach storage from outside `core`.
mod sealed {
    /// Carries the byte-level `write`/`read` operations `core`'s own
    /// per-target [`super::SnapshotStore`] impls provide.
    ///
    /// `async fn` in this trait is deliberate, not an oversight: the trait
    /// is internal to `core` (never exported as `dyn Sealed`, never
    /// crossing an FFI boundary), so the auto-trait-bound loss the
    /// `async_fn_in_trait` lint warns about does not apply here — nothing
    /// needs a `Send` bound on the trait itself, and the `wasm32`
    /// `IndexedDbSnapshotStore` impl genuinely cannot offer one (`JsValue`
    /// is `!Send`).
    #[allow(async_fn_in_trait)]
    pub trait Sealed {
        /// The error type surfaced by this store's underlying medium.
        type Error: std::fmt::Debug;

        /// Atomically replaces the stored snapshot bytes: this call either
        /// fully publishes `bytes` or leaves the previously published
        /// snapshot intact, even if the process crashes mid-write.
        async fn write(&self, bytes: Vec<u8>) -> Result<(), Self::Error>;

        /// Reads the current snapshot bytes, or `None` if nothing has been
        /// written yet.
        async fn read(&self) -> Result<Option<Vec<u8>>, Self::Error>;
    }
}

/// One atomic snapshot slot.
///
/// Each compile target gets its own implementation of this trait
/// ([`MemorySnapshotStore`], [`FsSnapshotStore`], [`IndexedDbSnapshotStore`]).
/// Callers use [`save_snapshot`]/[`load_snapshot`]/[`load_snapshot_at_version`]
/// rather than `write`/`read` directly — those are the only entry points that
/// require a [`Persistable`] payload, which is what keeps secret material
/// unrepresentable. `write` and `read` themselves live on the private
/// [`sealed::Sealed`] supertrait, so this trait cannot be implemented, and its
/// byte-level methods cannot be called, from outside this crate.
pub trait SnapshotStore: sealed::Sealed {}

impl<T: sealed::Sealed> SnapshotStore for T {}

/// Errors from [`save_snapshot`]/[`load_snapshot`]/
/// [`load_snapshot_at_version`]: either the underlying store's medium failed,
/// or `serde_json` failed to (de)serialise the envelope.
#[derive(Debug)]
pub enum SnapshotError<E> {
    /// The underlying store's medium (disk, IndexedDB, ...) failed.
    Store(E),
    /// `serde_json` failed to serialise the envelope for writing.
    Serialize(serde_json::Error),
    /// `serde_json` failed to deserialise the stored bytes.
    Deserialize(serde_json::Error),
}

impl<E: std::fmt::Debug> std::fmt::Display for SnapshotError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl<E: std::fmt::Debug> std::error::Error for SnapshotError<E> {}

/// Wraps `payload` in an [`Envelope`] with `schema_version` and `as_of`, and
/// atomically writes it through `store`.
///
/// Only types that implement [`Persistable`] — an explicit, opt-in marker —
/// can reach this function. That is the snapshot store's API shape that
/// makes storing secret material unrepresentable (ADR-0004/ADR-0005): a
/// credential type would have to deliberately implement `Persistable` to
/// become storable here, and none does.
///
/// The payload is taken **by reference**: serialisation only ever reads it,
/// and a caller that has just built a candidate value it may or may not
/// install (see [`crate::sync::SyncCycle`]) should not have to clone it to
/// write it. The `T: Persistable` bound is still on `T` itself, so the
/// sealed opt-in is unchanged — `&T: Serialize` is what does the work.
///
/// ```compile_fail
/// # use hummingbird_core::storage::{save_snapshot, MemorySnapshotStore};
/// # async fn example() {
/// let store = MemorySnapshotStore::default();
/// // `String` never implements `Persistable` — this must not compile.
/// save_snapshot(&store, 1, 0, &"a secret token".to_string()).await.unwrap();
/// # }
/// ```
pub async fn save_snapshot<T, S>(
    store: &S,
    schema_version: u32,
    as_of: u64,
    payload: &T,
) -> Result<(), SnapshotError<S::Error>>
where
    T: Persistable,
    S: SnapshotStore,
{
    // Built field-wise rather than through `Envelope::new`, which carries the
    // by-value `T: Persistable` bound; the fields are `pub`, and `&T:
    // Serialize` is all serialisation needs.
    let envelope = Envelope {
        schema_version,
        as_of,
        payload,
    };
    let bytes = serde_json::to_vec(&envelope).map_err(SnapshotError::Serialize)?;
    store.write(bytes).await.map_err(SnapshotError::Store)
}

/// Reads and deserialises the current [`Envelope`], or `None` if nothing has
/// been written yet.
///
/// Deliberately version-blind: the payload is deserialised into `T` whatever
/// `schema_version` says, so a caller that reads the version off the returned
/// envelope only ever sees versions whose payload still *parses* as `T`. For
/// a caller whose answer to a stale version is "discard it", that is the
/// wrong order of operations — use [`load_snapshot_at_version`], which
/// decides on the version before the payload is ever parsed.
pub async fn load_snapshot<T, S>(
    store: &S,
) -> Result<Option<Envelope<T>>, SnapshotError<S::Error>>
where
    T: Persistable,
    S: SnapshotStore,
{
    match store.read().await.map_err(SnapshotError::Store)? {
        None => Ok(None),
        Some(bytes) => {
            let envelope =
                serde_json::from_slice(&bytes).map_err(SnapshotError::Deserialize)?;
            Ok(Some(envelope))
        }
    }
}

/// [`load_snapshot`] for a caller that discards anything not written at
/// `expected_version`: `Ok(None)` covers both "nothing stored" and "stored at
/// another version", and the payload is only ever parsed as `T` once the
/// version matches.
///
/// **The version is read before the payload, and that ordering is the whole
/// point.** Checking `envelope.schema_version` on a [`load_snapshot`] result
/// cannot work for the failure it exists to catch: the commonest schema
/// change is a *new field*, and a stale snapshot missing it fails
/// `serde_json` deserialisation outright — so the load errors before the
/// caller's version check can discard it, and a version bump meant to make
/// the upgrade silent instead bricks the device on boot until its storage is
/// cleared by hand. Parsing the envelope's own two scalar fields first, and
/// the payload only on a match, is what makes a bump of
/// [`crate::sync::SYNC_MIRROR_SCHEMA_VERSION`] survivable regardless of how
/// the payload's shape moved.
pub async fn load_snapshot_at_version<T, S>(
    store: &S,
    expected_version: u32,
) -> Result<Option<Envelope<T>>, SnapshotError<S::Error>>
where
    T: Persistable,
    S: SnapshotStore,
{
    match store.read().await.map_err(SnapshotError::Store)? {
        None => Ok(None),
        Some(bytes) => {
            // The payload stays raw for this first parse — `serde_json::Value`
            // rather than `T` — so a shape mismatch cannot fail the read
            // before the version has been consulted.
            let envelope: Envelope<serde_json::Value> =
                serde_json::from_slice(&bytes).map_err(SnapshotError::Deserialize)?;
            if envelope.schema_version != expected_version {
                return Ok(None);
            }
            let payload =
                serde_json::from_value(envelope.payload).map_err(SnapshotError::Deserialize)?;
            Ok(Some(Envelope {
                schema_version: envelope.schema_version,
                as_of: envelope.as_of,
                payload,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TestPayload {
        note: String,
        count: u32,
    }

    impl PersistableSealed for TestPayload {}
    impl Persistable for TestPayload {}

    #[tokio::test]
    async fn envelope_round_trips_through_serde_json_with_version_and_as_of_intact() {
        let store = MemorySnapshotStore::default();
        let payload = TestPayload {
            note: "hello".to_string(),
            count: 3,
        };

        save_snapshot(&store, 7, 1_700_000_000_000, &payload)
            .await
            .unwrap();

        let loaded: Envelope<TestPayload> = load_snapshot(&store).await.unwrap().unwrap();

        assert_eq!(loaded.schema_version, 7);
        assert_eq!(loaded.as_of, 1_700_000_000_000);
        assert_eq!(loaded.payload, payload);
    }

    #[tokio::test]
    async fn load_before_any_save_is_none() {
        let store = MemorySnapshotStore::default();
        let loaded: Option<Envelope<TestPayload>> = load_snapshot(&store).await.unwrap();
        assert_eq!(loaded, None);
    }

    #[test]
    fn envelope_bytes_are_readable_json() {
        // ADR-0003: "a readable mirror file is worth real money when
        // debugging" — the envelope must actually be human-readable JSON,
        // not merely something `serde_json` can round-trip.
        let envelope = Envelope::new(
            1,
            0,
            TestPayload {
                note: "hello".to_string(),
                count: 3,
            },
        );
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("\"schema_version\":1"));
        assert!(json.contains("\"as_of\":0"));
        assert!(json.contains("\"note\":\"hello\""));
    }
}
