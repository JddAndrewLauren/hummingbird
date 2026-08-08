//! Maps one Google `events.list` item ([`RawEvent`]) to the provider-agnostic
//! [`EventRecord`] (issue #70). No Google shape or field name survives past
//! this function.

use chrono::{NaiveDate, TimeZone, Utc};
use std::fmt;

use crate::calendar::event::{EventRecord, EventStatus, EventTime};

use super::raw::{RawEvent, RawEventDateTime};

/// A raw Google event item this module cannot map onto [`EventRecord`].
/// Any one of these aborts the whole snapshot build (see `super::adapter`).
#[derive(Debug)]
pub enum MapError {
    UnknownStatus {
        event_id: String,
        status: String,
    },
    MissingBoundary {
        event_id: String,
        field: &'static str,
    },
    MissingOriginalStartTime {
        event_id: String,
    },
    InvalidDate {
        event_id: String,
        value: String,
    },
    InvalidDateTime {
        event_id: String,
        value: String,
    },
    EmptyBoundary {
        event_id: String,
        field: &'static str,
    },
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapError::UnknownStatus { event_id, status } => {
                write!(f, "event {event_id}: unknown status {status:?}")
            }
            MapError::MissingBoundary { event_id, field } => {
                write!(f, "event {event_id}: missing {field}")
            }
            MapError::MissingOriginalStartTime { event_id } => {
                write!(
                    f,
                    "event {event_id}: recurringEventId set but no originalStartTime"
                )
            }
            MapError::InvalidDate { event_id, value } => {
                write!(f, "event {event_id}: invalid date {value:?}")
            }
            MapError::InvalidDateTime { event_id, value } => {
                write!(f, "event {event_id}: invalid dateTime {value:?}")
            }
            MapError::EmptyBoundary { event_id, field } => {
                write!(f, "event {event_id}: {field} has neither date nor dateTime")
            }
        }
    }
}

impl std::error::Error for MapError {}

pub fn map_event(raw: &RawEvent, calendar_id: &str) -> Result<EventRecord, MapError> {
    let status = map_status(raw)?;

    // Cancelled recurring instances carry no `start`/`end`, only
    // `originalStartTime` — the slot the cancellation occupies. Everything
    // else uses `start`/`end` directly.
    let (start, end) = match (&raw.start, &raw.end) {
        (Some(start), Some(end)) => (
            map_event_date_time(start, &raw.id, "start")?,
            map_event_date_time(end, &raw.id, "end")?,
        ),
        (None, None) => {
            let original = raw.original_start_time.as_ref().ok_or_else(|| {
                MapError::MissingOriginalStartTime {
                    event_id: raw.id.clone(),
                }
            })?;
            let boundary = map_event_date_time(original, &raw.id, "originalStartTime")?;
            (boundary.clone(), boundary)
        }
        (Some(_), None) => {
            return Err(MapError::MissingBoundary {
                event_id: raw.id.clone(),
                field: "end",
            })
        }
        (None, Some(_)) => {
            return Err(MapError::MissingBoundary {
                event_id: raw.id.clone(),
                field: "start",
            })
        }
    };

    let all_day = start.time_zone.is_empty();

    let recurrence_id = match &raw.recurring_event_id {
        Some(series_id) => {
            let original = raw.original_start_time.as_ref().ok_or_else(|| {
                MapError::MissingOriginalStartTime {
                    event_id: raw.id.clone(),
                }
            })?;
            let identity = original
                .date
                .as_deref()
                .or(original.date_time.as_deref())
                .ok_or_else(|| MapError::EmptyBoundary {
                    event_id: raw.id.clone(),
                    field: "originalStartTime",
                })?;
            Some(format!("{series_id}@{identity}"))
        }
        None => None,
    };

    let organizer = raw.organizer.as_ref().and_then(|organizer| {
        organizer
            .display_name
            .clone()
            .or_else(|| organizer.email.clone())
    });

    let provider_updated_at_ms = match &raw.updated {
        Some(value) => parse_rfc3339_ms(value).ok_or_else(|| MapError::InvalidDateTime {
            event_id: raw.id.clone(),
            value: value.clone(),
        })?,
        None => 0,
    };

    Ok(EventRecord {
        provider_event_id: raw.id.clone(),
        calendar_id: calendar_id.to_string(),
        title: raw.summary.clone().unwrap_or_default(),
        start,
        end,
        all_day,
        recurrence_id,
        location: raw.location.clone(),
        organizer,
        status,
        provider_updated_at_ms,
        html_link: raw.html_link.clone(),
    })
}

fn map_status(raw: &RawEvent) -> Result<EventStatus, MapError> {
    match raw.status.as_str() {
        "confirmed" => Ok(EventStatus::Confirmed),
        "tentative" => Ok(EventStatus::Tentative),
        "cancelled" => Ok(EventStatus::Cancelled),
        other => Err(MapError::UnknownStatus {
            event_id: raw.id.clone(),
            status: other.to_string(),
        }),
    }
}

fn map_event_date_time(
    boundary: &RawEventDateTime,
    event_id: &str,
    field: &'static str,
) -> Result<EventTime, MapError> {
    if let Some(date_time) = &boundary.date_time {
        let instant_ms = parse_rfc3339_ms(date_time).ok_or_else(|| MapError::InvalidDateTime {
            event_id: event_id.to_string(),
            value: date_time.clone(),
        })?;
        let time_zone = boundary.time_zone.clone().unwrap_or_default();
        return Ok(EventTime::timed(instant_ms, time_zone));
    }

    if let Some(date) = &boundary.date {
        let parsed =
            NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| MapError::InvalidDate {
                event_id: event_id.to_string(),
                value: date.clone(),
            })?;
        let midnight = parsed
            .and_hms_opt(0, 0, 0)
            .expect("00:00:00 is always a valid time");
        let instant_ms = Utc.from_utc_datetime(&midnight).timestamp_millis();
        return Ok(EventTime::all_day(instant_ms));
    }

    Err(MapError::EmptyBoundary {
        event_id: event_id.to_string(),
        field,
    })
}

fn parse_rfc3339_ms(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::google::raw::RawOrganizer;

    fn timed_boundary(date_time: &str, time_zone: &str) -> RawEventDateTime {
        RawEventDateTime {
            date: None,
            date_time: Some(date_time.to_string()),
            time_zone: Some(time_zone.to_string()),
        }
    }

    fn all_day_boundary(date: &str) -> RawEventDateTime {
        RawEventDateTime {
            date: Some(date.to_string()),
            date_time: None,
            time_zone: None,
        }
    }

    fn base_event() -> RawEvent {
        RawEvent {
            id: "evt-1".to_string(),
            status: "confirmed".to_string(),
            summary: Some("Standup".to_string()),
            location: Some("Zoom".to_string()),
            organizer: Some(RawOrganizer {
                email: Some("john@twinion.net".to_string()),
                display_name: None,
            }),
            start: Some(timed_boundary(
                "2024-01-08T09:00:00-08:00",
                "America/Los_Angeles",
            )),
            end: Some(timed_boundary(
                "2024-01-08T09:30:00-08:00",
                "America/Los_Angeles",
            )),
            recurring_event_id: None,
            original_start_time: None,
            updated: Some("2024-01-01T00:00:00.000Z".to_string()),
            html_link: Some("https://calendar.google.com/event?eid=abc".to_string()),
        }
    }

    #[test]
    fn maps_a_timed_confirmed_event() {
        let record = map_event(&base_event(), "cal-primary").unwrap();
        assert_eq!(record.provider_event_id, "evt-1");
        assert_eq!(record.calendar_id, "cal-primary");
        assert_eq!(record.title, "Standup");
        assert!(!record.all_day);
        assert_eq!(record.status, EventStatus::Confirmed);
        assert_eq!(record.recurrence_id, None);
        assert_eq!(record.organizer.as_deref(), Some("john@twinion.net"));
    }

    #[test]
    fn all_day_date_only_boundary_maps_to_utc_midnight_with_no_time_zone() {
        let mut raw = base_event();
        raw.start = Some(all_day_boundary("2024-03-01"));
        raw.end = Some(all_day_boundary("2024-03-03"));

        let record = map_event(&raw, "cal-primary").unwrap();
        assert!(record.all_day);
        assert_eq!(record.start.time_zone, "");
        assert_eq!(record.end.time_zone, "");
        assert_eq!(
            record.start.instant_ms,
            Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
        // Exclusive end convention: end date is the day after the last day.
        assert_eq!(
            record.end.instant_ms,
            Utc.with_ymd_and_hms(2024, 3, 3, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
    }

    #[test]
    fn dst_spring_forward_offset_produces_correct_utc_instant() {
        // 2024-03-10 02:30 America/Los_Angeles does not exist (spring
        // forward at 02:00 -> 03:00); Google always sends the resolved
        // offset, so -07:00 (already-advanced) is what we receive.
        let mut raw = base_event();
        raw.start = Some(timed_boundary(
            "2024-03-10T03:30:00-07:00",
            "America/Los_Angeles",
        ));
        raw.end = Some(timed_boundary(
            "2024-03-10T04:00:00-07:00",
            "America/Los_Angeles",
        ));

        let record = map_event(&raw, "cal-primary").unwrap();
        assert_eq!(record.start.time_zone, "America/Los_Angeles");
        assert_eq!(
            record.start.instant_ms,
            chrono::DateTime::parse_from_rfc3339("2024-03-10T10:30:00Z")
                .unwrap()
                .timestamp_millis()
        );
    }

    #[test]
    fn cancelled_instance_without_start_end_maps_from_original_start_time() {
        let mut raw = base_event();
        raw.status = "cancelled".to_string();
        raw.summary = None;
        raw.start = None;
        raw.end = None;
        raw.recurring_event_id = Some("series-1".to_string());
        raw.original_start_time = Some(timed_boundary(
            "2024-01-15T09:00:00-08:00",
            "America/Los_Angeles",
        ));

        let record = map_event(&raw, "cal-primary").unwrap();
        assert_eq!(record.status, EventStatus::Cancelled);
        assert_eq!(record.start, record.end);
        assert_eq!(
            record.recurrence_id.as_deref(),
            Some("series-1@2024-01-15T09:00:00-08:00")
        );
    }

    #[test]
    fn recurring_instance_identity_combines_series_and_original_start() {
        let mut raw = base_event();
        raw.recurring_event_id = Some("series-1".to_string());
        raw.original_start_time = Some(timed_boundary(
            "2024-01-08T09:00:00-08:00",
            "America/Los_Angeles",
        ));

        let record = map_event(&raw, "cal-primary").unwrap();
        assert_eq!(
            record.recurrence_id.as_deref(),
            Some("series-1@2024-01-08T09:00:00-08:00")
        );
    }

    #[test]
    fn unknown_status_is_an_error() {
        let mut raw = base_event();
        raw.status = "needsAction".to_string();
        let err = map_event(&raw, "cal-primary").unwrap_err();
        assert!(matches!(err, MapError::UnknownStatus { .. }));
    }

    #[test]
    fn missing_start_and_end_without_original_start_time_is_an_error() {
        let mut raw = base_event();
        raw.start = None;
        raw.end = None;
        let err = map_event(&raw, "cal-primary").unwrap_err();
        assert!(matches!(err, MapError::MissingOriginalStartTime { .. }));
    }
}
