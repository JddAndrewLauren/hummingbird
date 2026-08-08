//! [`EventRecord`]: the provider-agnostic calendar event shape (issue #46's
//! field list), and its supporting types.

use serde::{Deserialize, Serialize};

/// One endpoint (start or end) of an event: an instant plus the IANA time
/// zone the provider associated with it.
///
/// For an all-day event's boundary, `instant_ms` is the UTC instant of
/// local midnight on the calendar date (the provider's `date`-only
/// boundaries have no time-of-day and no meaningful time zone); `time_zone`
/// is then the empty string. Multi-day all-day events use the provider's
/// exclusive-end convention, matching [`EventRecord::end`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventTime {
    /// Milliseconds since the Unix epoch, UTC.
    pub instant_ms: i64,
    /// IANA time zone identifier (e.g. `"America/Los_Angeles"`), or the
    /// empty string for an all-day boundary with no time-of-day.
    pub time_zone: String,
}

impl EventTime {
    /// A timed boundary: an instant in a named IANA time zone.
    pub fn timed(instant_ms: i64, time_zone: impl Into<String>) -> Self {
        Self {
            instant_ms,
            time_zone: time_zone.into(),
        }
    }

    /// An all-day boundary: the UTC instant of local midnight on the
    /// calendar date, with no time zone.
    pub fn all_day(midnight_utc_ms: i64) -> Self {
        Self {
            instant_ms: midnight_utc_ms,
            time_zone: String::new(),
        }
    }
}

/// The provider's confirmation state for an event.
///
/// Deliberately generic (not `GoogleEventStatus`): #47's future M365 adapter
/// targets this same enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    Confirmed,
    Tentative,
    Cancelled,
}

/// One calendar event instance, provider-agnostic throughout.
///
/// Carries issue #46's full field list: provider event id, calendar id,
/// title, start/end with time zones, all-day flag, recurrence identity,
/// location, organizer, status, provider update time, and HTML link. No
/// field or name here is Google-specific — this is the shared shape #47's
/// M365 adapter later fills too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRecord {
    /// The provider's own id for this expanded instance (recurring events
    /// expand to one id per instance).
    pub provider_event_id: String,
    /// The provider's id for the calendar this event belongs to.
    pub calendar_id: String,
    pub title: String,
    pub start: EventTime,
    pub end: EventTime,
    pub all_day: bool,
    /// Identifies which recurring series (and which instance of it) this
    /// event belongs to, or `None` for a non-recurring event.
    pub recurrence_id: Option<String>,
    pub location: Option<String>,
    pub organizer: Option<String>,
    pub status: EventStatus,
    /// Milliseconds since the Unix epoch, UTC: when the provider last
    /// updated this event.
    pub provider_updated_at_ms: i64,
    pub html_link: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event() -> EventRecord {
        EventRecord {
            provider_event_id: "evt-1".to_string(),
            calendar_id: "cal-primary".to_string(),
            title: "Standup".to_string(),
            start: EventTime::timed(1_700_000_000_000, "America/Los_Angeles"),
            end: EventTime::timed(1_700_000_600_000, "America/Los_Angeles"),
            all_day: false,
            recurrence_id: Some("series-1".to_string()),
            location: Some("Zoom".to_string()),
            organizer: Some("john@twinion.net".to_string()),
            status: EventStatus::Confirmed,
            provider_updated_at_ms: 1_699_999_000_000,
            html_link: Some("https://calendar.google.com/event?eid=abc".to_string()),
        }
    }

    #[test]
    fn event_record_round_trips_through_serde_json_with_every_field_intact() {
        let event = sample_event();
        let json = serde_json::to_string(&event).unwrap();
        let restored: EventRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, event);
    }
}
