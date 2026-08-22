//! Maps one Google `events.list` item ([`RawEvent`]) to the provider-agnostic
//! [`EventRecord`] (issue #70). No Google shape or field name survives past
//! this function.

use chrono::NaiveDate;
use std::fmt;

use crate::calendar::event::{EventRecord, EventStatus, EventWhen};

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
    /// One boundary is `date`-only and the other is a `dateTime`. Google
    /// never mixes them, and [`EventWhen`] cannot express it: an event is
    /// all-day or timed, never half of each. Loud rather than guessed.
    MixedBoundaries {
        event_id: String,
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
            MapError::MixedBoundaries { event_id } => {
                write!(
                    f,
                    "event {event_id}: one boundary is date-only and the other is a dateTime"
                )
            }
        }
    }
}

impl std::error::Error for MapError {}

/// Maps one raw item.
///
/// **No time zone enters or leaves this function** (ADR-0015's 2026-08-10
/// amendment): an all-day boundary's `date` is carried through as the
/// provider's own string, byte-identical, and a timed boundary's `dateTime`
/// already carries its own offset, so the resolved UTC instant is exact
/// without one. The page-level `timeZone` `events.list` reports is
/// deliberately *not* a parameter any more — it existed only to resolve
/// all-day dates to local-midnight instants, which is the flattening the
/// amendment forbids.
///
/// `Ok(None)` means the item is a deleted *standalone* event: a tombstone
/// with nothing to place it by (see the `(None, None)` arm below). Every
/// other unmappable shape is an `Err` that aborts the snapshot.
pub fn map_event(raw: &RawEvent, calendar_id: &str) -> Result<Option<EventRecord>, MapError> {
    let status = map_status(raw)?;

    // Cancelled recurring instances carry no `start`/`end`, only
    // `originalStartTime` — the slot the cancellation occupies. Everything
    // else uses `start`/`end` directly.
    let when = match (&raw.start, &raw.end) {
        (Some(start), Some(end)) => map_when(start, end, &raw.id)?,
        (None, None) => {
            // A deleted standalone event: `showDeleted=true` returns it
            // stripped to little more than `id` and `status: cancelled`,
            // with no `originalStartTime` either — there is no instant to
            // place it at and, being cancelled, nothing any query would
            // return (see `crate::calendar::query`). Dropping it is right;
            // erroring would let one deleted event abort a whole snapshot.
            if status == EventStatus::Cancelled && raw.original_start_time.is_none() {
                return Ok(None);
            }
            let original = raw.original_start_time.as_ref().ok_or_else(|| {
                MapError::MissingOriginalStartTime {
                    event_id: raw.id.clone(),
                }
            })?;
            // A zero-length span at the slot the cancellation occupies —
            // `start == end` on either arm, which no half-open membership
            // test can match. That is the point: only `originalStartTime`
            // exists to place it, and no read query hands a cancellation
            // back anyway.
            map_when(original, original, &raw.id)?
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

    Ok(Some(EventRecord {
        provider_event_id: raw.id.clone(),
        calendar_id: calendar_id.to_string(),
        title: raw.summary.clone().unwrap_or_default(),
        when,
        recurrence_id,
        location: raw.location.clone(),
        organizer,
        status,
        provider_updated_at_ms,
        html_link: raw.html_link.clone(),
        description: raw.description.clone(),
    }))
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

/// The two boundaries decided **together**, which is what [`EventWhen`]
/// requires: all-day-ness is a property of the event, not of one endpoint.
/// It is read off the raw shape (`date` vs `dateTime` presence), never off
/// anything derived — a timed boundary carries no `timeZone` of its own
/// whenever the calendar's default zone applies, and that must not be
/// mistaken for an all-day event.
fn map_when(
    start: &RawEventDateTime,
    end: &RawEventDateTime,
    event_id: &str,
) -> Result<EventWhen, MapError> {
    match (
        boundary_kind(start, event_id, "start")?,
        boundary_kind(end, event_id, "end")?,
    ) {
        (Boundary::Date(start_date), Boundary::Date(end_date)) => {
            // Byte-identical, both of them: the provider's own civil dates
            // and its own exclusive-end convention, carried through
            // untouched. Parsing is *validation only* — a malformed date
            // would silently break the lexicographic membership tests
            // `query.rs` runs over these strings.
            validate_date(&start_date, event_id)?;
            validate_date(&end_date, event_id)?;
            Ok(EventWhen::AllDay {
                start_date,
                end_date,
            })
        }
        (Boundary::DateTime(start_ms), Boundary::DateTime(end_ms)) => {
            Ok(EventWhen::Timed { start_ms, end_ms })
        }
        _ => Err(MapError::MixedBoundaries {
            event_id: event_id.to_string(),
        }),
    }
}

enum Boundary {
    Date(String),
    DateTime(i64),
}

fn boundary_kind(
    boundary: &RawEventDateTime,
    event_id: &str,
    field: &'static str,
) -> Result<Boundary, MapError> {
    if let Some(date_time) = &boundary.date_time {
        let instant_ms = parse_rfc3339_ms(date_time).ok_or_else(|| MapError::InvalidDateTime {
            event_id: event_id.to_string(),
            value: date_time.clone(),
        })?;
        return Ok(Boundary::DateTime(instant_ms));
    }
    if let Some(date) = &boundary.date {
        return Ok(Boundary::Date(date.clone()));
    }
    Err(MapError::EmptyBoundary {
        event_id: event_id.to_string(),
        field,
    })
}

fn validate_date(date: &str, event_id: &str) -> Result<(), MapError> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| MapError::InvalidDate {
            event_id: event_id.to_string(),
            value: date.to_string(),
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

    fn timed_boundary(date_time: &str) -> RawEventDateTime {
        RawEventDateTime {
            date: None,
            date_time: Some(date_time.to_string()),
        }
    }

    fn all_day_boundary(date: &str) -> RawEventDateTime {
        RawEventDateTime {
            date: Some(date.to_string()),
            date_time: None,
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
            start: Some(timed_boundary("2024-01-08T09:00:00-08:00")),
            end: Some(timed_boundary("2024-01-08T09:30:00-08:00")),
            recurring_event_id: None,
            original_start_time: None,
            updated: Some("2024-01-01T00:00:00.000Z".to_string()),
            html_link: Some("https://calendar.google.com/event?eid=abc".to_string()),
            description: Some("Coffee and standup notes.".to_string()),
        }
    }

    /// `map_event` for the items that do map — the `Ok(None)` tombstone case
    /// has its own test below.
    fn map_ok(raw: &RawEvent, calendar_id: &str) -> EventRecord {
        map_event(raw, calendar_id)
            .expect("maps without error")
            .expect("is not a tombstone")
    }

    #[test]
    fn maps_a_timed_confirmed_event() {
        let record = map_ok(&base_event(), "cal-primary");
        assert_eq!(record.provider_event_id, "evt-1");
        assert_eq!(record.calendar_id, "cal-primary");
        assert_eq!(record.title, "Standup");
        assert!(!record.when.is_all_day());
        assert_eq!(record.status, EventStatus::Confirmed);
        assert_eq!(record.recurrence_id, None);
        assert_eq!(record.organizer.as_deref(), Some("john@twinion.net"));
    }

    #[test]
    fn a_timed_boundarys_own_offset_resolves_the_instant_with_no_zone_stored() {
        let record = map_ok(&base_event(), "cal-primary");
        assert_eq!(
            record.when,
            EventWhen::Timed {
                start_ms: chrono::DateTime::parse_from_rfc3339("2024-01-08T17:00:00Z")
                    .unwrap()
                    .timestamp_millis(),
                end_ms: chrono::DateTime::parse_from_rfc3339("2024-01-08T17:30:00Z")
                    .unwrap()
                    .timestamp_millis(),
            }
        );
    }

    #[test]
    fn an_all_day_events_dates_are_carried_through_byte_identical() {
        // ADR-0015's amendment, and the defect it names: this event must
        // NOT become a pair of instants resolved in any zone. The mapper
        // has no zone to resolve against any more — that is the point.
        let mut raw = base_event();
        raw.start = Some(all_day_boundary("2026-09-09"));
        raw.end = Some(all_day_boundary("2026-09-16"));

        let record = map_ok(&raw, "cal-primary");
        assert_eq!(
            record.when,
            EventWhen::AllDay {
                start_date: "2026-09-09".to_string(),
                end_date: "2026-09-16".to_string(),
            }
        );
    }

    #[test]
    fn the_exclusive_end_date_is_the_providers_own_never_adjusted() {
        // A one-day all-day event: Google states the end as the NEXT day,
        // and that convention is preserved rather than normalised here.
        let mut raw = base_event();
        raw.start = Some(all_day_boundary("2024-03-01"));
        raw.end = Some(all_day_boundary("2024-03-02"));

        assert_eq!(
            map_ok(&raw, "cal-primary").when,
            EventWhen::all_day("2024-03-01", "2024-03-02")
        );
    }

    #[test]
    fn a_malformed_all_day_date_is_an_error_not_a_string_that_breaks_comparisons() {
        // These strings are compared lexicographically by `query.rs`, so a
        // shape that is not `YYYY-MM-DD` silently answers the wrong
        // membership question rather than failing.
        let mut raw = base_event();
        raw.start = Some(all_day_boundary("09/09/2026"));
        raw.end = Some(all_day_boundary("2026-09-16"));

        let err = map_event(&raw, "cal-primary").unwrap_err();
        assert!(matches!(err, MapError::InvalidDate { .. }));
    }

    #[test]
    fn one_date_only_boundary_and_one_date_time_boundary_is_an_error() {
        let mut raw = base_event();
        raw.start = Some(all_day_boundary("2024-03-01"));
        // `end` stays the timed boundary from `base_event`.

        let err = map_event(&raw, "cal-primary").unwrap_err();
        assert!(matches!(err, MapError::MixedBoundaries { .. }));
    }

    #[test]
    fn a_date_time_boundary_is_never_all_day() {
        // All-day-ness is read off `date` vs `dateTime` presence and
        // nothing else — never off anything derived, which is how a timed
        // event on the calendar's default zone (Google omits the optional
        // per-boundary `timeZone` for those) used to risk being misread.
        let mut raw = base_event();
        raw.start = Some(timed_boundary("2024-01-08T09:00:00-08:00"));
        raw.end = Some(timed_boundary("2024-01-08T09:30:00-08:00"));

        let record = map_ok(&raw, "cal-primary");
        assert!(!record.when.is_all_day());
        assert_eq!(
            record.when,
            EventWhen::Timed {
                start_ms: chrono::DateTime::parse_from_rfc3339("2024-01-08T17:00:00Z")
                    .unwrap()
                    .timestamp_millis(),
                end_ms: chrono::DateTime::parse_from_rfc3339("2024-01-08T17:30:00Z")
                    .unwrap()
                    .timestamp_millis(),
            }
        );
    }

    #[test]
    fn dst_spring_forward_offset_produces_correct_utc_instant() {
        // 2024-03-10 02:30 America/Los_Angeles does not exist (spring
        // forward at 02:00 -> 03:00); Google always sends the resolved
        // offset, so -07:00 (already-advanced) is what we receive — and
        // that offset is all the instant ever needed.
        let mut raw = base_event();
        raw.start = Some(timed_boundary("2024-03-10T03:30:00-07:00"));
        raw.end = Some(timed_boundary("2024-03-10T04:00:00-07:00"));

        let record = map_ok(&raw, "cal-primary");
        assert_eq!(
            record.when,
            EventWhen::Timed {
                start_ms: chrono::DateTime::parse_from_rfc3339("2024-03-10T10:30:00Z")
                    .unwrap()
                    .timestamp_millis(),
                end_ms: chrono::DateTime::parse_from_rfc3339("2024-03-10T11:00:00Z")
                    .unwrap()
                    .timestamp_millis(),
            }
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
        raw.original_start_time = Some(timed_boundary("2024-01-15T09:00:00-08:00"));

        let record = map_ok(&raw, "cal-primary");
        assert_eq!(record.status, EventStatus::Cancelled);
        let EventWhen::Timed { start_ms, end_ms } = record.when else {
            panic!("a timed originalStartTime maps to the timed arm");
        };
        assert_eq!(start_ms, end_ms);
        assert_eq!(
            record.recurrence_id.as_deref(),
            Some("series-1@2024-01-15T09:00:00-08:00")
        );
    }

    #[test]
    fn a_cancelled_all_day_instance_is_a_zero_length_date_span() {
        let mut raw = base_event();
        raw.status = "cancelled".to_string();
        raw.start = None;
        raw.end = None;
        raw.recurring_event_id = Some("series-1".to_string());
        raw.original_start_time = Some(all_day_boundary("2024-01-15"));

        let record = map_ok(&raw, "cal-primary");
        assert_eq!(record.when, EventWhen::all_day("2024-01-15", "2024-01-15"));
    }

    #[test]
    fn recurring_instance_identity_combines_series_and_original_start() {
        let mut raw = base_event();
        raw.recurring_event_id = Some("series-1".to_string());
        raw.original_start_time = Some(timed_boundary("2024-01-08T09:00:00-08:00"));

        let record = map_ok(&raw, "cal-primary");
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

    #[test]
    fn a_deleted_standalone_event_is_skipped_rather_than_aborting_the_snapshot() {
        // What `showDeleted=true` returns for a deleted non-recurring event:
        // id and status, nothing to place it by. Erroring here would let a
        // single deleted event take out a whole poll.
        let mut raw = base_event();
        raw.status = "cancelled".to_string();
        raw.summary = None;
        raw.start = None;
        raw.end = None;
        raw.original_start_time = None;

        assert_eq!(map_event(&raw, "cal-primary").unwrap(), None);
    }
}
