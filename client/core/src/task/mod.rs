//! The task mirror contract: the app's own domain model, the reconciling
//! local read model built from it, and the read-time queries over it.
//!
//! This module is the cash value of ADR-0001 seam rule 1 — "the app's schema
//! is the domain model, never Linear's Issue shape". Nothing here names a
//! Linear field, state id, or label id; the adapter that does lands in a
//! later slice and translates into these types at the seam. The
//! `no_linear_vocabulary_leaks_into_the_domain_model` test in
//! `tests/task_mirror_golden.rs` enforces that mechanically rather than by
//! convention, the same way `core`'s binding-agnostic rule is enforced.
//!
//! Persistence is #68's: [`Mirror`] opts into [`crate::storage::Persistable`]
//! and rides `save_snapshot`/`load_snapshot`, so its atomic-write and
//! `as_of` guarantees come for free.

pub mod deadletter;
pub mod item;
pub mod mirror;
pub mod query;
pub mod route;

pub use deadletter::{DeadLetterEntry, DeadLetterJournal, DeadLetterReason};
pub use item::{Item, ItemId, Presence, Priority, Stage};
pub use mirror::{Mirror, ReconcileReport, Sweep, MIRROR_SCHEMA_VERSION};
pub use query::by_priority_then_due;
pub use route::Route;
