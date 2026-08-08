//! Maps one Google `events.list` item ([`RawEvent`]) to the provider-agnostic
//! [`EventRecord`] (issue #70). No Google shape or field name survives past
//! this function.

use chrono::{NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
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
    UnknownTimeZone {
        event_id: String,
        time_zone: String,
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
            MapError::UnknownTimeZone {
                event_id,
                time_zone,
            } => {
                write!(f, "event {event_id}: unknown time zone {time_zone:?}")
            }
        }
    }
}

impl std::error::Error for MapError {}

/// Maps one raw item. `calendar_time_zone` is the page-level `timeZone`
/// `events.list` reports for the calendar — the anchor every all-day
/// boundary on the page is resolved against (see [`map_event_date_time`]).
/// `None` (Google omitted it) falls back to UTC.
///
/// `Ok(None)` means the item is a deleted *standalone* event: a tombstone
/// with nothing to place it by (see the `(None, None)` arm below). Every
/// other unmappable shape is an `Err` that aborts the snapshot.
pub fn map_event(
    raw: &RawEvent,
    calendar_id: &str,
    calendar_time_zone: Option<&str>,
) -> Result<Option<EventRecord>, MapError> {
    let status = map_status(raw)?;

    // Cancelled recurring instances carry no `start`/`end`, only
    // `originalStartTime` — the slot the cancellation occupies. Everything
    // else uses `start`/`end` directly.
    let (start, end, all_day) = match (&raw.start, &raw.end) {
        (Some(start), Some(end)) => (
            map_event_date_time(start, &raw.id, "start", calendar_time_zone)?,
            map_event_date_time(end, &raw.id, "end", calendar_time_zone)?,
            // All-day-ness comes from the raw shape (`date` vs `dateTime`
            // presence on the start boundary), never from the mapped
            // time_zone string — a timed `dateTime` boundary carries no
            // `timeZone` of its own whenever the calendar's default zone
            // applies, and that must not be mistaken for an all-day event.
            start.date.is_some(),
        ),
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
            let boundary =
                map_event_date_time(original, &raw.id, "originalStartTime", calendar_time_zone)?;
            (boundary.clone(), boundary, original.date.is_some())
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
        start,
        end,
        all_day,
        recurrence_id,
        location: raw.location.clone(),
        organizer,
        status,
        provider_updated_at_ms,
        html_link: raw.html_link.clone(),
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

fn map_event_date_time(
    boundary: &RawEventDateTime,
    event_id: &str,
    field: &'static str,
    calendar_time_zone: Option<&str>,
) -> Result<EventTime, MapError> {
    if let Some(date_time) = &boundary.date_time {
        let instant_ms = parse_rfc3339_ms(date_time).ok_or_else(|| MapError::InvalidDateTime {
            event_id: event_id.to_string(),
            value: date_time.clone(),
        })?;
        // Google omits the optional per-boundary `timeZone` whenever the
        // event sits in the calendar's own zone, sending only an offset —
        // and an offset is not a zone (it cannot say what this event's wall
        // clock reads next November). The page-level zone IS the calendar's
        // zone, so it is the right answer here rather than an empty string;
        // only a page that reported no zone at all leaves this unknown.
        let time_zone = match &boundary.time_zone {
            Some(zone) => zone.clone(),
            None => match calendar_time_zone {
                Some(name) => resolve_time_zone(Some(name), event_id)?.name().to_string(),
                None => String::new(),
            },
        };
        return Ok(EventTime::timed(instant_ms, time_zone));
    }

    if let Some(date) = &boundary.date {
        let parsed =
            NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| MapError::InvalidDate {
                event_id: event_id.to_string(),
                value: date.clone(),
            })?;
        // Midnight *in the calendar's zone*, not UTC. A date-only boundary
        // names a local day, and the two are not interchangeable: read as
        // UTC, an America/Los_Angeles all-day event starts at 17:00 the
        // previous afternoon and ends 17:00 early, so "is this happening
        // now" and "what is next" both answer for the wrong day.
        let zone = resolve_time_zone(calendar_time_zone, event_id)?;
        let instant_ms = local_midnight_ms(parsed, zone);
        return Ok(EventTime::all_day(instant_ms, zone.name()));
    }

    Err(MapError::EmptyBoundary {
        event_id: event_id.to_string(),
        field,
    })
}

/// The calendar's zone, or UTC when `events.list` omitted `timeZone`. An
/// unparseable zone is an error rather than a silent UTC fallback: falling
/// back would reintroduce exactly the off-by-an-offset this function exists
/// to prevent, and quietly, which is the part that made it hard to see.
fn resolve_time_zone(calendar_time_zone: Option<&str>, event_id: &str) -> Result<Tz, MapError> {
    match calendar_time_zone {
        None => Ok(Tz::UTC),
        Some(name) => name.parse::<Tz>().map_err(|_| MapError::UnknownTimeZone {
            event_id: event_id.to_string(),
            time_zone: name.to_string(),
        }),
    }
}

/// The instant of local midnight on `date` in `zone`.
///
/// DST makes "midnight" occasionally not a single instant. Where the local
/// day starts twice (a fall-back overlap) the earlier instant is the start
/// of the day. Where it starts not at all — a spring-forward gap landing on
/// 00:00, as in America/Santiago or Asia/Beirut — the day begins at the
/// first instant that does exist, which is what walking forward finds.
fn local_midnight_ms(date: NaiveDate, zone: Tz) -> i64 {
    for hour in 0..=3 {
        let local = date
            .and_hms_opt(hour, 0, 0)
            .expect("hours 0..=3 are always valid times");
        if let Some(resolved) = zone.from_local_datetime(&local).earliest() {
            return resolved.timestamp_millis();
        }
    }
    // Unreachable for every zone in the IANA database (no DST gap is
    // anywhere near four hours), but a wrong answer beats a panic in a
    // mapper whose failure aborts a whole snapshot.
    Utc.from_utc_datetime(
        &date
            .and_hms_opt(0, 0, 0)
            .expect("00:00:00 is always a valid time"),
    )
    .timestamp_millis()
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

    /// The page-level `timeZone` the fixtures' calendar reports. Every test
    /// here maps against a real, offset-carrying zone rather than UTC —
    /// mapping all-day boundaries in UTC was the bug, and a UTC fixture
    /// cannot tell right from wrong.
    const CALENDAR_TZ: Option<&str> = Some("America/Los_Angeles");

    fn timed_boundary(date_time: &str, time_zone: &str) -> RawEventDateTime {
        RawEventDateTime {
            date: None,
            date_time: Some(date_time.to_string()),
            time_zone: Some(time_zone.to_string()),
        }
    }

    fn timed_boundary_no_zone(date_time: &str) -> RawEventDateTime {
        RawEventDateTime {
            date: None,
            date_time: Some(date_time.to_string()),
            time_zone: None,
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

    /// `map_event` for the items that do map — the `Ok(None)` tombstone case
    /// has its own test below.
    fn map_ok(raw: &RawEvent, calendar_id: &str, calendar_time_zone: Option<&str>) -> EventRecord {
        map_event(raw, calendar_id, calendar_time_zone)
            .expect("maps without error")
            .expect("is not a tombstone")
    }

    #[test]
    fn maps_a_timed_confirmed_event() {
        let record = map_ok(&base_event(), "cal-primary", CALENDAR_TZ);
        assert_eq!(record.provider_event_id, "evt-1");
        assert_eq!(record.calendar_id, "cal-primary");
        assert_eq!(record.title, "Standup");
        assert!(!record.all_day);
        assert_eq!(record.status, EventStatus::Confirmed);
        assert_eq!(record.recurrence_id, None);
        assert_eq!(record.organizer.as_deref(), Some("john@twinion.net"));
    }

    #[test]
    fn all_day_date_only_boundary_maps_to_local_midnight_in_the_calendars_zone() {
        let mut raw = base_event();
        raw.start = Some(all_day_boundary("2024-03-01"));
        raw.end = Some(all_day_boundary("2024-03-03"));

        let record = map_ok(&raw, "cal-primary", CALENDAR_TZ);
        assert!(record.all_day);
        assert_eq!(record.start.time_zone, "America/Los_Angeles");
        assert_eq!(record.end.time_zone, "America/Los_Angeles");
        // 2024-03-01 00:00 PST is 08:00Z — NOT 2024-03-01T00:00Z, which is
        // the previous afternoon locally and what this used to produce.
        assert_eq!(
            record.start.instant_ms,
            Utc.with_ymd_and_hms(2024, 3, 1, 8, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
        // Exclusive end convention: end date is the day after the last day.
        assert_eq!(
            record.end.instant_ms,
            Utc.with_ymd_and_hms(2024, 3, 3, 8, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
    }

    #[test]
    fn all_day_boundary_falls_back_to_utc_when_the_page_omits_a_time_zone() {
        let mut raw = base_event();
        raw.start = Some(all_day_boundary("2024-03-01"));
        raw.end = Some(all_day_boundary("2024-03-02"));

        let record = map_ok(&raw, "cal-primary", None);
        assert_eq!(record.start.time_zone, "UTC");
        assert_eq!(
            record.start.instant_ms,
            Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
    }

    #[test]
    fn all_day_boundary_crossing_a_dst_change_uses_each_days_own_offset() {
        // The US spring-forward is 2024-03-10. An all-day event spanning it
        // starts at 08:00Z (PST, UTC-8) and ends at 07:00Z (PDT, UTC-7): a
        // fixed offset applied to both boundaries would be wrong for one of
        // them, which is the whole reason this goes through a zone database
        // rather than an offset.
        let mut raw = base_event();
        raw.start = Some(all_day_boundary("2024-03-09"));
        raw.end = Some(all_day_boundary("2024-03-11"));

        let record = map_ok(&raw, "cal-primary", CALENDAR_TZ);
        assert_eq!(
            record.start.instant_ms,
            Utc.with_ymd_and_hms(2024, 3, 9, 8, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
        assert_eq!(
            record.end.instant_ms,
            Utc.with_ymd_and_hms(2024, 3, 11, 7, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
    }

    #[test]
    fn all_day_boundary_on_a_day_with_no_local_midnight_uses_the_first_real_instant() {
        // America/Santiago's 2024 DST change moves 2024-09-08 00:00 to
        // 01:00, so that local day has no midnight at all. The day still
        // starts — at 01:00 local, 04:00Z.
        let mut raw = base_event();
        raw.start = Some(all_day_boundary("2024-09-08"));
        raw.end = Some(all_day_boundary("2024-09-09"));

        let record = map_ok(&raw, "cal-primary", Some("America/Santiago"));
        assert_eq!(
            record.start.instant_ms,
            Utc.with_ymd_and_hms(2024, 9, 8, 4, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
    }

    #[test]
    fn an_unparseable_calendar_time_zone_is_an_error_not_a_silent_utc_fallback() {
        let mut raw = base_event();
        raw.start = Some(all_day_boundary("2024-03-01"));
        raw.end = Some(all_day_boundary("2024-03-02"));

        let err = map_event(&raw, "cal-primary", Some("Mars/Olympus_Mons")).unwrap_err();
        assert!(matches!(err, MapError::UnknownTimeZone { .. }));
    }

    #[test]
    fn timed_boundary_with_omitted_time_zone_is_not_all_day() {
        // Google routinely omits the optional `timeZone` field for events on
        // the calendar's default zone; that must not be mistaken for an
        // all-day (`date`-only) boundary.
        let mut raw = base_event();
        raw.start = Some(timed_boundary_no_zone("2024-01-08T09:00:00-08:00"));
        raw.end = Some(timed_boundary_no_zone("2024-01-08T09:30:00-08:00"));

        let record = map_ok(&raw, "cal-primary", CALENDAR_TZ);
        assert!(!record.all_day);
    }

    #[test]
    fn timed_boundary_with_omitted_time_zone_inherits_the_calendars_zone() {
        // The omission means "the calendar's own zone", and the page reports
        // that zone — dropping it would leave a timed event carrying only an
        // offset, which #71 requires start/end zones over.
        let mut raw = base_event();
        raw.start = Some(timed_boundary_no_zone("2024-01-08T09:00:00-08:00"));
        raw.end = Some(timed_boundary_no_zone("2024-01-08T09:30:00-08:00"));

        let record = map_ok(&raw, "cal-primary", CALENDAR_TZ);
        assert_eq!(record.start.time_zone, "America/Los_Angeles");
        assert_eq!(record.end.time_zone, "America/Los_Angeles");
    }

    #[test]
    fn timed_boundary_zone_stays_unknown_when_neither_boundary_nor_page_names_one() {
        // Nothing to inherit: an empty zone here is honest rather than a
        // guess (the instant itself is still exact — it came from the
        // offset).
        let mut raw = base_event();
        raw.start = Some(timed_boundary_no_zone("2024-01-08T09:00:00-08:00"));
        raw.end = Some(timed_boundary_no_zone("2024-01-08T09:30:00-08:00"));

        let record = map_ok(&raw, "cal-primary", None);
        assert_eq!(record.start.time_zone, "");
    }

    #[test]
    fn an_explicit_boundary_time_zone_wins_over_the_page_zone() {
        let mut raw = base_event();
        raw.start = Some(timed_boundary(
            "2024-01-08T12:00:00-05:00",
            "America/New_York",
        ));
        raw.end = Some(timed_boundary(
            "2024-01-08T12:30:00-05:00",
            "America/New_York",
        ));

        let record = map_ok(&raw, "cal-primary", CALENDAR_TZ);
        assert_eq!(record.start.time_zone, "America/New_York");
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

        let record = map_ok(&raw, "cal-primary", CALENDAR_TZ);
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

        let record = map_ok(&raw, "cal-primary", CALENDAR_TZ);
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

        let record = map_ok(&raw, "cal-primary", CALENDAR_TZ);
        assert_eq!(
            record.recurrence_id.as_deref(),
            Some("series-1@2024-01-08T09:00:00-08:00")
        );
    }

    #[test]
    fn unknown_status_is_an_error() {
        let mut raw = base_event();
        raw.status = "needsAction".to_string();
        let err = map_event(&raw, "cal-primary", CALENDAR_TZ).unwrap_err();
        assert!(matches!(err, MapError::UnknownStatus { .. }));
    }

    #[test]
    fn missing_start_and_end_without_original_start_time_is_an_error() {
        let mut raw = base_event();
        raw.start = None;
        raw.end = None;
        let err = map_event(&raw, "cal-primary", CALENDAR_TZ).unwrap_err();
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

        assert_eq!(map_event(&raw, "cal-primary", CALENDAR_TZ).unwrap(), None);
    }
}
