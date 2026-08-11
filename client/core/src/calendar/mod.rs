//! The provider-agnostic calendar mirror contract (issue #70): an event
//! record, a persisted snapshot of expanded instances, and the two read
//! queries every consumer (`/next-up-personal` #74, a future morning brief)
//! targets.
//!
//! Nothing here is Google-specific — this is the shared shape #71's Google
//! adapter fills and #47's future M365 adapter fills too. This module is
//! read-only context: it cannot mint, modify, or reference an Action in the
//! task authority (ADR-0002 rule 1).

mod event;
pub mod google;
mod query;
mod snapshot;

pub use event::{EventRecord, EventStatus, EventTime};
pub use query::{current_or_next_event, events_overlapping_interval, CurrentOrNext, Interval};
pub(crate) use query::is_actionable;
pub use google::{CalendarHorizon, CalendarSelection};
pub use snapshot::CalendarSnapshot;
