//! The Google Calendar adapter (issue #71): fills #70's provider-agnostic
//! contract from `calendars/{id}/events`. Nothing outside this module ever
//! sees a Google shape — [`adapter::fetch_calendar_snapshot`] is the only
//! public entry point, and it returns #70's [`crate::calendar::CalendarSnapshot`].

mod adapter;
mod map;
mod provider_poller;
mod raw;
mod reqwest_transport;
mod transport;

pub use adapter::{fetch_calendar_snapshot, AdapterError};
pub use map::MapError;
pub use provider_poller::GoogleProviderPoller;
pub use reqwest_transport::ReqwestEventsTransport;
pub use transport::{EventsTransport, TransportError};
