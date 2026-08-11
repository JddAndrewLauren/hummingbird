//! Parsing one Microsoft Graph `event` resource (a `calendarView/delta`
//! item) into a typed [`CalendarItem`]. Everything downstream
//! (`calendar_event.rs`) reads only this type, never the raw JSON —
//! `calendar_poll::calendar_event`'s own split, for Graph's shape.
//!
//! **Every request this poller makes carries `Prefer: outlook.timezone="UTC"`**
//! (`main.rs`'s job to set the header) — Graph's default `start`/`end`
//! shape carries a Windows time-zone name (e.g. `"Pacific Standard Time"`),
//! not an IANA one, and resolving that correctly needs Windows' own zone
//! database, which this crate has no business carrying (`city-waste`'s own
//! "needs a tzdb, cannot be derived from a day number" reasoning, one step
//! further — Windows zone names are not even in `jiff`'s IANA tzdb at
//! all). Asking Graph for UTC sidesteps the whole problem: every
//! `dateTime` this module parses is already UTC wall-clock text, parsed as
//! a naive civil datetime and pinned to the UTC zone directly, never
//! through an offset or a zone name this module would otherwise have to
//! resolve.
//!
//! **`original_start` (the second half of #158's
//! `m365_calendar_v1_key(id, series_master_id, original_start)`) is
//! populated from the occurrence's own Graph `id`, not a timestamp.**
//! Unlike Google's `event` resource, Graph's `event` resource carries no
//! `originalStartTime`-equivalent field at all — there is nothing to read.
//! What Graph *does* document as stable is the occurrence's own `id`: an
//! expanded `calendarView` occurrence keeps the same `id` across a
//! reschedule of that specific instance, which is exactly the invariant
//! #158's key recipe needs (a reschedule must land on the row already
//! minted for it, never mint a second). Using the occurrence `id` in the
//! `original_start` position satisfies that invariant using the one fact
//! Graph actually guarantees stable, rather than inventing a field Graph
//! does not provide — the same category of documented, unconfirmed
//! assumption `server/city-waste/src/page.rs`'s module header records for
//! its own "still open" reading, to be checked against a live tenant's
//! first real reschedule (see this crate's own `lib.rs` doc and the
//! issue-137 thread).

use std::fmt;

/// Why an event body could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalendarItemError {
    NotJson(String),
    MissingField(&'static str),
    BadTimestamp(&'static str),
}

impl fmt::Display for CalendarItemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CalendarItemError::NotJson(m) => write!(f, "event response is not JSON: {m}"),
            CalendarItemError::MissingField(field) => write!(f, "event is missing `{field}`"),
            CalendarItemError::BadTimestamp(field) => write!(f, "event's `{field}` is not a readable date/time"),
        }
    }
}

/// The Graph event fields `calendar_event.rs` needs, decoded once.
#[derive(Debug, Clone, PartialEq)]
pub struct CalendarItem {
    pub id: String,
    /// Present only on an occurrence/exception of a recurring series —
    /// `None` for a standalone event, in which case `id` itself is the
    /// series identity (`hummingbird_domain::m365_calendar_v1_key`'s own
    /// fallback).
    pub series_master_id: Option<String>,
    pub subject: String,
    pub body_preview: String,
    pub web_link: Option<String>,
    pub organizer_email: Option<String>,
    pub attendee_emails: Vec<String>,
    pub is_all_day: bool,
    pub starts_at_ms: i64,
    pub ends_at_ms: i64,
}

/// The result of parsing one delta-query item: a live event, or Graph's
/// own "this item left the collection" marker (`"@removed"`,
/// `mail_message.rs`'s own reason for the analogous mail case).
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedCalendarItem {
    Live(Box<CalendarItem>),
    Removed(String),
}

/// Parses one `calendarView/delta` item body — requested with `Prefer:
/// outlook.timezone="UTC"` (this module's own doc), so `start`/`end`
/// `dateTime` values are always UTC wall-clock text.
pub fn parse_calendar_item(json: &str) -> Result<ParsedCalendarItem, CalendarItemError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| CalendarItemError::NotJson(e.to_string()))?;
    let object = value.as_object().ok_or_else(|| CalendarItemError::NotJson("not an object".into()))?;

    let id = object.get("id").and_then(|v| v.as_str()).map(str::to_string).ok_or(CalendarItemError::MissingField("id"))?;

    if object.contains_key("@removed") {
        return Ok(ParsedCalendarItem::Removed(id));
    }

    let starts_at_ms = parse_utc_datetime(object.get("start"), "start")?;
    let ends_at_ms = parse_utc_datetime(object.get("end"), "end")?;

    let subject = object.get("subject").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let body_preview = object.get("bodyPreview").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let web_link = object.get("webLink").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(str::to_string);
    let organizer_email = object
        .get("organizer")
        .and_then(|o| o.get("emailAddress"))
        .and_then(|e| e.get("address"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let series_master_id = object.get("seriesMasterId").and_then(|v| v.as_str()).map(str::to_string);
    let is_all_day = object.get("isAllDay").and_then(|v| v.as_bool()).unwrap_or(false);
    let attendee_emails = object
        .get("attendees")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|e| e.get("emailAddress")?.get("address")?.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    Ok(ParsedCalendarItem::Live(Box::new(CalendarItem {
        id,
        series_master_id,
        subject,
        body_preview,
        web_link,
        organizer_email,
        attendee_emails,
        is_all_day,
        starts_at_ms,
        ends_at_ms,
    })))
}

/// Reads a `start`/`end` object's `dateTime` as UTC wall-clock text (this
/// module's own doc — every request carries `Prefer: outlook.timezone="UTC"`,
/// so there is never an offset or a non-UTC zone name to resolve).
fn parse_utc_datetime(v: Option<&serde_json::Value>, field: &'static str) -> Result<i64, CalendarItemError> {
    let raw = v
        .and_then(|v| v.get("dateTime"))
        .and_then(|v| v.as_str())
        .ok_or(CalendarItemError::MissingField(field))?;
    let civil: jiff::civil::DateTime = raw.parse().map_err(|_| CalendarItemError::BadTimestamp(field))?;
    let zoned = civil.to_zoned(jiff::tz::TimeZone::UTC).map_err(|_| CalendarItemError::BadTimestamp(field))?;
    Ok(zoned.timestamp().as_millisecond())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event_json(id: &str, extra: &str) -> String {
        format!(
            r#"{{
                "id": "{id}",
                "subject": "Board review",
                "bodyPreview": "quarterly numbers",
                "webLink": "https://outlook.office.com/calendar/item/abc",
                "start": {{"dateTime": "2026-08-15T09:00:00.0000000", "timeZone": "UTC"}},
                "end": {{"dateTime": "2026-08-15T09:30:00.0000000", "timeZone": "UTC"}}
                {extra}
            }}"#
        )
    }

    #[test]
    fn a_live_event_parses_every_core_field() {
        let json = event_json("evt-1", "");
        let parsed = parse_calendar_item(&json).expect("well-formed event parses");
        let ParsedCalendarItem::Live(evt) = parsed else { panic!("expected Live") };
        assert_eq!(evt.id, "evt-1");
        assert_eq!(evt.subject, "Board review");
        assert!(!evt.is_all_day);
        assert_eq!(evt.series_master_id, None);
        assert_eq!(evt.starts_at_ms, 1_786_784_400_000);
        assert_eq!(evt.ends_at_ms, 1_786_786_200_000);
    }

    #[test]
    fn a_removed_item_is_named_not_parsed_as_a_full_event() {
        let json = r#"{"id": "evt-1", "@removed": {"reason": "deleted"}}"#;
        assert_eq!(parse_calendar_item(json), Ok(ParsedCalendarItem::Removed("evt-1".to_string())));
    }

    #[test]
    fn an_all_day_event_is_flagged_directly_from_the_field() {
        let json = event_json("evt-1", r#", "isAllDay": true"#);
        let ParsedCalendarItem::Live(evt) = parse_calendar_item(&json).unwrap() else { panic!() };
        assert!(evt.is_all_day);
    }

    #[test]
    fn an_occurrence_carries_its_series_master_id() {
        let json = event_json("evt-1_20260815", r#", "seriesMasterId": "series-abc""#);
        let ParsedCalendarItem::Live(evt) = parse_calendar_item(&json).unwrap() else { panic!() };
        assert_eq!(evt.series_master_id.as_deref(), Some("series-abc"));
    }

    #[test]
    fn organizer_and_attendees_are_read() {
        let json = event_json(
            "evt-1",
            r#", "organizer": {"emailAddress": {"address": "boss@contoso.com"}},
                "attendees": [{"emailAddress": {"address": "me@contoso.com"}}]"#,
        );
        let ParsedCalendarItem::Live(evt) = parse_calendar_item(&json).unwrap() else { panic!() };
        assert_eq!(evt.organizer_email.as_deref(), Some("boss@contoso.com"));
        assert_eq!(evt.attendee_emails, vec!["me@contoso.com".to_string()]);
    }

    #[test]
    fn missing_id_is_named() {
        assert_eq!(parse_calendar_item(r#"{"subject": "s"}"#), Err(CalendarItemError::MissingField("id")));
    }

    #[test]
    fn missing_start_is_named() {
        let json = r#"{"id": "evt-1", "end": {"dateTime": "2026-08-15T09:30:00.0000000", "timeZone": "UTC"}}"#;
        assert_eq!(parse_calendar_item(json), Err(CalendarItemError::MissingField("start")));
    }

    #[test]
    fn not_json_is_named() {
        assert!(matches!(parse_calendar_item("not json"), Err(CalendarItemError::NotJson(_))));
    }
}
