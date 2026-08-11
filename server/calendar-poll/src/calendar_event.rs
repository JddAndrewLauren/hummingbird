//! Parsing one Google Calendar API event resource (`events.list`'s own
//! per-item shape) into a typed [`CalendarEvent`]. Everything downstream
//! (`event.rs`, `busy.rs`) reads only this type, never the raw JSON, the
//! same split `gmail-poll::message` draws for `GmailMessage`.

use std::fmt;

/// Why an event body could not be read — named rather than a quietly
/// skipped event, the same "malformed with a reason" discipline
/// `hummingbird_domain::EnvelopeProblem` and `gmail_poll::MessageError` use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalendarEventError {
    NotJson(String),
    MissingField(&'static str),
    BadTimestamp(&'static str),
}

impl fmt::Display for CalendarEventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CalendarEventError::NotJson(m) => write!(f, "event response is not JSON: {m}"),
            CalendarEventError::MissingField(field) => write!(f, "event is missing `{field}`"),
            CalendarEventError::BadTimestamp(field) => write!(f, "event's `{field}` is not a readable date/time"),
        }
    }
}

/// The Calendar fields `event.rs` and `busy.rs` need, decoded once.
#[derive(Debug, Clone, PartialEq)]
pub struct CalendarEvent {
    pub id: String,
    /// Present only on an instance of a recurring series — `None` for a
    /// standalone event, in which case `id` itself is the series identity
    /// (`hummingbird_domain::google_calendar_v1_key`'s own fallback).
    pub recurring_event_id: Option<String>,
    /// The instance's originally scheduled start, verbatim as Google wrote
    /// it (an RFC3339 `dateTime` or a bare `date`) — Google's own
    /// `originalStartTime` on a recurring instance, else this event's own
    /// `start`. #158's `google_calendar_v1_key` recipe's second half; a
    /// later reschedule must not change it, which is why this is read from
    /// `originalStartTime` rather than `start` whenever Google provides it.
    pub original_start_time: String,
    pub summary: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub html_link: Option<String>,
    pub organizer_email: Option<String>,
    pub attendee_emails: Vec<String>,
    pub is_all_day: bool,
    pub starts_at_ms: i64,
    pub ends_at_ms: i64,
    /// `false` for Google's own "opaque" (the default when the field is
    /// absent — busy is the default) or anything other than the literal
    /// "transparent"; the brief's "transparent / free" exclusion reads this
    /// flag, never the raw string, so a future Google value cannot silently
    /// slip past the exclusion.
    pub is_transparent: bool,
    /// The operator's own RSVP, resolved from the attendee entry carrying
    /// `"self": true`. `None` when the operator organized the event and is
    /// not listed as their own attendee (Google omits a self-attendee entry
    /// for a solo/organizer-only event) — the brief's "declined" exclusion
    /// must not read that absence as a decline.
    pub self_response_status: Option<String>,
}

/// The result of parsing one event body: a live event, or Google's own
/// "this instance was deleted" marker inside an incremental sync page
/// (`status: "cancelled"`, carrying only `id`/`status`/`etag`/`kind` — never
/// enough to build a full [`CalendarEvent`]). [`ParsedCalendarEvent::Cancelled`]
/// is named separately from [`CalendarEventError`] because it is not a
/// parse failure: it is the expected shape for a deletion, and must be
/// skipped loudly but non-fatally rather than logged as malformed.
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedCalendarEvent {
    Live(Box<CalendarEvent>),
    Cancelled(String),
}

/// Parses one `events.list` item body.
pub fn parse_calendar_event(json: &str) -> Result<ParsedCalendarEvent, CalendarEventError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| CalendarEventError::NotJson(e.to_string()))?;
    let object = value.as_object().ok_or_else(|| CalendarEventError::NotJson("not an object".into()))?;

    let id = object
        .get("id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or(CalendarEventError::MissingField("id"))?;

    let status = object.get("status").and_then(|v| v.as_str()).unwrap_or("confirmed");
    if status == "cancelled" {
        return Ok(ParsedCalendarEvent::Cancelled(id));
    }

    let start = parse_time_point(object.get("start"), "start")?;
    let end = parse_time_point(object.get("end"), "end")?;
    let original_start_time = object
        .get("originalStartTime")
        .map(|v| parse_time_point(Some(v), "originalStartTime"))
        .transpose()?
        .map(|tp| tp.raw)
        .unwrap_or_else(|| start.raw.clone());

    let summary = object.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let description = non_empty_text(object.get("description"));
    let location = non_empty_text(object.get("location"));
    let html_link = non_empty_text(object.get("htmlLink"));
    let organizer_email =
        object.get("organizer").and_then(|o| o.get("email")).and_then(|v| v.as_str()).map(str::to_string);
    let recurring_event_id = object.get("recurringEventId").and_then(|v| v.as_str()).map(str::to_string);

    let transparency = object.get("transparency").and_then(|v| v.as_str()).unwrap_or("opaque");
    let is_transparent = transparency == "transparent";

    let attendees: Vec<&serde_json::Value> =
        object.get("attendees").and_then(|v| v.as_array()).map(|a| a.iter().collect()).unwrap_or_default();
    let attendee_emails = attendees
        .iter()
        .filter_map(|a| a.get("email")?.as_str())
        .map(str::to_string)
        .collect();
    let self_response_status = attendees
        .iter()
        .find(|a| a.get("self").and_then(|v| v.as_bool()) == Some(true))
        .and_then(|a| a.get("responseStatus"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    Ok(ParsedCalendarEvent::Live(Box::new(CalendarEvent {
        id,
        recurring_event_id,
        original_start_time,
        summary,
        description,
        location,
        html_link,
        organizer_email,
        attendee_emails,
        is_all_day: start.is_all_day,
        starts_at_ms: start.ms,
        ends_at_ms: end.ms,
        is_transparent,
        self_response_status,
    })))
}

fn non_empty_text(v: Option<&serde_json::Value>) -> Option<String> {
    v.and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(str::to_string)
}

struct TimePoint {
    ms: i64,
    raw: String,
    is_all_day: bool,
}

/// Reads one `start`/`end`/`originalStartTime` object — either a timed
/// `{"dateTime": "...", "timeZone": "..."}` or an all-day `{"date": "..."}`
/// (Google's own two shapes; never both on the same object).
fn parse_time_point(v: Option<&serde_json::Value>, field: &'static str) -> Result<TimePoint, CalendarEventError> {
    let object = v.and_then(|v| v.as_object()).ok_or(CalendarEventError::MissingField(field))?;
    if let Some(date_time) = object.get("dateTime").and_then(|v| v.as_str()) {
        let ts: jiff::Timestamp =
            date_time.parse().map_err(|_| CalendarEventError::BadTimestamp(field))?;
        return Ok(TimePoint { ms: ts.as_millisecond(), raw: date_time.to_string(), is_all_day: false });
    }
    if let Some(date) = object.get("date").and_then(|v| v.as_str()) {
        let civil: jiff::civil::Date = date.parse().map_err(|_| CalendarEventError::BadTimestamp(field))?;
        let zoned = civil
            .to_zoned(jiff::tz::TimeZone::UTC)
            .map_err(|_| CalendarEventError::BadTimestamp(field))?;
        return Ok(TimePoint { ms: zoned.timestamp().as_millisecond(), raw: date.to_string(), is_all_day: true });
    }
    Err(CalendarEventError::MissingField(field))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timed_event(id: &str, extra: &str) -> String {
        format!(
            r#"{{
                "id": "{id}",
                "status": "confirmed",
                "summary": "Standup",
                "htmlLink": "https://calendar.google.com/event?eid=abc",
                "start": {{"dateTime": "2026-08-15T09:00:00-07:00"}},
                "end": {{"dateTime": "2026-08-15T09:30:00-07:00"}}
                {extra}
            }}"#
        )
    }

    #[test]
    fn a_timed_event_parses_every_core_field() {
        let json = timed_event("evt-1", "");
        let parsed = parse_calendar_event(&json).expect("well-formed event parses");
        let ParsedCalendarEvent::Live(evt) = parsed else { panic!("expected Live") };
        assert_eq!(evt.id, "evt-1");
        assert_eq!(evt.summary, "Standup");
        assert!(!evt.is_all_day);
        assert!(!evt.is_transparent, "opaque is the default");
        assert_eq!(evt.self_response_status, None);
        assert_eq!(evt.recurring_event_id, None);
        assert_eq!(evt.original_start_time, "2026-08-15T09:00:00-07:00");
    }

    #[test]
    fn a_cancelled_event_is_named_not_parsed_as_a_full_event() {
        let json = r#"{"id": "evt-1", "status": "cancelled", "etag": "\"abc\"", "kind": "calendar#event"}"#;
        assert_eq!(parse_calendar_event(json), Ok(ParsedCalendarEvent::Cancelled("evt-1".to_string())));
    }

    #[test]
    fn an_all_day_event_is_flagged_and_dated_at_utc_midnight() {
        let json = r#"{
            "id": "evt-1", "status": "confirmed", "summary": "Vacation",
            "start": {"date": "2026-08-15"}, "end": {"date": "2026-08-16"}
        }"#;
        let ParsedCalendarEvent::Live(evt) = parse_calendar_event(json).unwrap() else { panic!() };
        assert!(evt.is_all_day);
        assert_eq!(evt.starts_at_ms, 1_786_752_000_000);
    }

    #[test]
    fn transparency_transparent_is_flagged_free() {
        let json = timed_event("evt-1", r#", "transparency": "transparent""#);
        let ParsedCalendarEvent::Live(evt) = parse_calendar_event(&json).unwrap() else { panic!() };
        assert!(evt.is_transparent);
    }

    #[test]
    fn the_operators_own_response_status_is_read_off_the_self_attendee() {
        let json = timed_event(
            "evt-1",
            r#", "attendees": [
                {"email": "other@x.com", "responseStatus": "accepted"},
                {"email": "me@x.com", "self": true, "responseStatus": "declined"}
            ]"#,
        );
        let ParsedCalendarEvent::Live(evt) = parse_calendar_event(&json).unwrap() else { panic!() };
        assert_eq!(evt.self_response_status.as_deref(), Some("declined"));
        assert_eq!(evt.attendee_emails, vec!["other@x.com".to_string(), "me@x.com".to_string()]);
    }

    #[test]
    fn no_self_attendee_is_none_not_declined() {
        let json = timed_event("evt-1", r#", "attendees": [{"email": "other@x.com", "responseStatus": "accepted"}]"#);
        let ParsedCalendarEvent::Live(evt) = parse_calendar_event(&json).unwrap() else { panic!() };
        assert_eq!(evt.self_response_status, None);
    }

    #[test]
    fn a_recurring_instance_carries_its_series_id_and_original_start() {
        let json = timed_event(
            "evt-1_20260815",
            r#", "recurringEventId": "series-abc", "originalStartTime": {"dateTime": "2026-08-15T09:00:00-07:00"}"#,
        );
        let ParsedCalendarEvent::Live(evt) = parse_calendar_event(&json).unwrap() else { panic!() };
        assert_eq!(evt.recurring_event_id.as_deref(), Some("series-abc"));
        assert_eq!(evt.original_start_time, "2026-08-15T09:00:00-07:00");
    }

    #[test]
    fn missing_id_is_named() {
        assert_eq!(parse_calendar_event(r#"{"status": "confirmed"}"#), Err(CalendarEventError::MissingField("id")));
    }

    #[test]
    fn missing_start_is_named() {
        let json = r#"{"id": "evt-1", "status": "confirmed", "end": {"dateTime": "2026-08-15T09:30:00-07:00"}}"#;
        assert_eq!(parse_calendar_event(json), Err(CalendarEventError::MissingField("start")));
    }

    #[test]
    fn not_json_is_named() {
        assert!(matches!(parse_calendar_event("not json"), Err(CalendarEventError::NotJson(_))));
    }
}
