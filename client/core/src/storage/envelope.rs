//! [`Envelope`]: the persisted-snapshot wrapper, and [`Persistable`]: the
//! opt-in marker that gates what can go through it.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Sealing boundary for [`Persistable`], the same C-SEALED pattern
/// [`super::SnapshotStore`] uses. `PersistableSealed` is `pub` but lives in
/// a module reachable only through a `pub(crate)` re-export
/// ([`super::PersistableSealed`]), so no crate outside `core` can name it,
/// implement it, or therefore implement `Persistable`.
pub(crate) mod sealed {
    /// Carries no operations: being nameable is the whole point.
    pub trait PersistableSealed {}
}

/// Marker for domain payload types eligible for snapshot persistence.
///
/// Deliberately opt-in rather than a blanket impl over `Serialize +
/// DeserializeOwned`: this is the API shape that makes storing secret
/// material unrepresentable (ADR-0004/ADR-0005 — no credential is ever
/// persisted). A type has to declare, in one place, that writing it to disk
/// or IndexedDB is fine. No credential type ever does.
///
/// Sealed, so "opt-in" is enforced rather than merely offered. Unsealed, the
/// property held only for types `core` itself defines: any downstream crate
/// could wrap a credential, implement `Persistable` for the wrapper, and
/// hand it to the public `save_snapshot` — with the compile-fail test on
/// `String` still passing, because that test only proves a type *without* an
/// impl is rejected, not that no such impl can be written. Sealing is what
/// makes the guarantee a property of the API's shape.
///
/// The cost is deliberate: a new persistable payload — #47's M365 mirror,
/// say — is a change inside `core`, next to the other snapshot types, and
/// not something a host crate can add on its own.
pub trait Persistable: Serialize + DeserializeOwned + sealed::PersistableSealed {}

/// A persisted snapshot: schema version, "as of" timestamp, and the domain
/// payload — the export named in ADR-0001 rule 3.
///
/// `as_of` is caller-supplied (milliseconds since the Unix epoch, by
/// convention) rather than sampled internally, so the envelope stays
/// deterministic and testable and never calls a clock that panics on bare
/// `wasm32-unknown-unknown`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope<T> {
    /// The schema version of `payload`.
    pub schema_version: u32,
    /// Milliseconds since the Unix epoch, supplied by the caller.
    pub as_of: u64,
    /// The domain payload. Schemas are deliberately not fixed by this crate.
    pub payload: T,
}

impl<T: Persistable> Envelope<T> {
    /// Wraps `payload` with a schema version and an "as of" timestamp.
    pub fn new(schema_version: u32, as_of: u64, payload: T) -> Self {
        Self {
            schema_version,
            as_of,
            payload,
        }
    }
}
