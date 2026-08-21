//! The provider-agnostic calendar mirror contract (issue #70): an event
//! record, a persisted snapshot of expanded instances, and the two read
//! queries every consumer (`/next-up-personal` #74, a future morning brief)
//! targets.
//!
//! Nothing here is Google-specific except [`google`] itself and [`host`],
//! the per-host wrapper over one Google poller (#73, moved here from
//! `ffi-web` by #564) — the rest is the shared shape #71's Google adapter
//! fills and #47's future M365 adapter fills too. This module is
//! read-only context: it cannot mint, modify, or reference an Action in the
//! task authority (ADR-0002 rule 1).

mod event;
pub mod google;
// The host wraps the `reqwest`-backed Google transport, so it exists only in
// builds that have one — `client/next-up` compiles this crate with
// `default-features = false` for `rank()` alone (see `core/Cargo.toml`'s
// `[features]`), and there is no transport for it to hold there.
#[cfg(feature = "reqwest-transport")]
mod host;
mod query;
pub mod selection;
mod snapshot;

pub use event::{EventRecord, EventStatus, EventWhen};
#[cfg(feature = "reqwest-transport")]
pub use host::{
    outcome_name, CalendarEventsResponse, CalendarHostCore, CalendarListResponse,
    CALENDAR_POLL_INTERVAL_MS,
};
pub(crate) use query::is_actionable;
pub use google::{CalendarHorizon, CalendarSelection};
pub use selection::effective_selection;
pub use query::{current_or_next_event, events_overlapping_interval, CurrentOrNext, Interval};
pub use snapshot::CalendarSnapshot;
