//! Raw shapes of the Google Calendar `events.list` response
//! (`calendar/v3/calendars/{calendarId}/events`). These structs exist only
//! to `serde_json::from_str` a page body; nothing here crosses out of the
//! `google` module — [`super::map::map_event`] is the only consumer.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RawEventsPage {
    #[serde(default)]
    pub items: Vec<RawEvent>,
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
    // The page's own `timeZone` is deliberately NOT transcribed: it
    // existed here only to resolve all-day boundaries to local-midnight
    // instants, which ADR-0015's 2026-08-10 amendment forbids (an all-day
    // event keeps the provider's civil dates verbatim). Nothing reads it,
    // so nothing parses it.
}

#[derive(Debug, Deserialize)]
pub struct RawEvent {
    pub id: String,
    pub status: String,
    pub summary: Option<String>,
    pub location: Option<String>,
    pub organizer: Option<RawOrganizer>,
    pub start: Option<RawEventDateTime>,
    pub end: Option<RawEventDateTime>,
    #[serde(rename = "recurringEventId")]
    pub recurring_event_id: Option<String>,
    #[serde(rename = "originalStartTime")]
    pub original_start_time: Option<RawEventDateTime>,
    pub updated: Option<String>,
    #[serde(rename = "htmlLink")]
    pub html_link: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RawOrganizer {
    pub email: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
}

/// Google's `EventDateTime`: either a `date` (all-day) or a `dateTime`
/// (timed, carrying its own UTC offset). Exactly one of `date`/`date_time`
/// is expected to be present per the API contract; [`super::map`] treats
/// neither being present as malformed input.
///
/// The boundary's optional `timeZone` is deliberately not transcribed
/// either: a `dateTime` already carries the offset that fixes its instant,
/// and a zone is exactly what ADR-0015's amendment says must not be stored.
#[derive(Debug, Deserialize)]
pub struct RawEventDateTime {
    pub date: Option<String>,
    #[serde(rename = "dateTime")]
    pub date_time: Option<String>,
}
