//! The Google Calendar adapter (issue #71): fills #70's provider-agnostic
//! contract from `calendars/{id}/events`. Nothing outside this module ever
//! sees a Google shape — [`adapter::fetch_calendar_snapshot`] is the only
//! public entry point, and it returns #70's [`crate::calendar::CalendarSnapshot`].
//!
//! [`calendar_list::list_calendars`] is the one read here that is not part of
//! the mirror contract: the picker's options (#73). It is in the core for the
//! same reason everything else is — ADR-0003 gives every client one HTTP path.

mod adapter;
mod calendar_list;
mod map;
mod provider_poller;
mod raw;
#[cfg(feature = "reqwest-transport")]
mod reqwest_transport;
mod transport;

pub use adapter::{fetch_calendar_snapshot, AdapterError, CalendarHorizon, CalendarSelection};
pub use calendar_list::{list_calendars, CalendarListEntry, CalendarListError};
pub use map::MapError;
pub use provider_poller::GoogleProviderPoller;
#[cfg(feature = "reqwest-transport")]
pub use reqwest_transport::ReqwestGoogleTransport;
pub use transport::{CalendarListTransport, EventsTransport, TransportError};
