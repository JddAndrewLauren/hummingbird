//! Mapping one [`CalendarEvent`] onto the ADR-0013 `calendar_event`-kind
//! [`Event`] shape the rule engine evaluates against
//! (`hummingbird_domain::EVENT_KINDS`).

use std::collections::BTreeMap;

use hummingbird_domain::{google_calendar_v1_key, now_as_deadline, Event, FieldValue, GOOGLE_CALENDAR_V1};

use crate::calendar_event::CalendarEvent;

/// This poller polls exactly one calendar (the operator's primary one) —
/// there is no per-event "which calendar" fact to read off the API
/// response the way `organizer`/`attendees` are, so the `calendar_event`
/// kind's own `calendar` field is this fixed literal rather than a
/// parameter with nothing to vary it. A multi-calendar poll is out of
/// scope (the brief names one address, the same way `city-waste` names
/// one).
const CALENDAR_ID: &str = "primary";

/// Builds the `calendar_event`-kind [`Event`] one Calendar API event
/// presents to the rule engine. Pure: no clock read beyond what
/// `evt.starts_at_ms` already carries.
pub fn calendar_event_to_event(evt: &CalendarEvent) -> Event {
    let title = if evt.summary.is_empty() { "(untitled event)".to_string() } else { evt.summary.clone() };
    let starts_at = now_as_deadline(evt.starts_at_ms);
    let ends_at = now_as_deadline(evt.ends_at_ms);

    let mut extras = BTreeMap::new();
    extras.insert("calendar".to_string(), FieldValue::Str(CALENDAR_ID.to_string()));
    extras.insert("title".to_string(), FieldValue::Str(title.clone()));
    extras.insert("organizer".to_string(), FieldValue::Str(evt.organizer_email.clone().unwrap_or_default()));
    extras.insert("location".to_string(), FieldValue::Str(evt.location.clone().unwrap_or_default()));
    extras.insert("attendees".to_string(), FieldValue::StrList(evt.attendee_emails.clone()));
    extras.insert("starts_at".to_string(), FieldValue::Str(starts_at.clone()));
    extras.insert("ends_at".to_string(), FieldValue::Str(ends_at));
    extras.insert("is_all_day".to_string(), FieldValue::Bool(evt.is_all_day));
    extras.insert("response".to_string(), FieldValue::Str(evt.self_response_status.clone().unwrap_or_default()));

    Event {
        source: GOOGLE_CALENDAR_V1.to_string(),
        source_key: google_calendar_v1_key(&evt.id, evt.recurring_event_id.as_deref(), &evt.original_start_time),
        occurred_at: starts_at,
        title,
        body: evt.description.clone(),
        url: evt.html_link.clone(),
        severity: None,
        calendar_busy: None,
        event_kind: Some("calendar_event".to_string()),
        extras,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evt() -> CalendarEvent {
        CalendarEvent {
            id: "evt-1".to_string(),
            recurring_event_id: None,
            original_start_time: "2026-08-15T09:00:00-07:00".to_string(),
            summary: "Board review".to_string(),
            description: Some("quarterly numbers".to_string()),
            location: Some("Room 4".to_string()),
            html_link: Some("https://calendar.google.com/event?eid=abc".to_string()),
            organizer_email: Some("boss@example.com".to_string()),
            attendee_emails: vec!["boss@example.com".to_string(), "me@example.com".to_string()],
            is_all_day: false,
            starts_at_ms: 1_786_871_400_000,
            ends_at_ms: 1_786_875_000_000,
            is_transparent: false,
            self_response_status: Some("accepted".to_string()),
        }
    }

    #[test]
    fn the_core_fields_are_populated_from_the_event() {
        let event = calendar_event_to_event(&evt());
        assert_eq!(event.source, "google-calendar/v1");
        assert_eq!(event.source_key, "evt-1:2026-08-15T09:00:00-07:00");
        assert_eq!(event.title, "Board review");
        assert_eq!(event.body.as_deref(), Some("quarterly numbers"));
        assert_eq!(event.event_kind.as_deref(), Some("calendar_event"));
        assert_eq!(event.url.as_deref(), Some("https://calendar.google.com/event?eid=abc"));
    }

    #[test]
    fn a_missing_summary_falls_back_rather_than_an_empty_title() {
        let mut e = evt();
        e.summary = String::new();
        let event = calendar_event_to_event(&e);
        assert_eq!(event.title, "(untitled event)");
    }

    #[test]
    fn a_recurring_instance_keys_on_the_series_id_and_original_start() {
        let mut e = evt();
        e.recurring_event_id = Some("series-abc".to_string());
        let event = calendar_event_to_event(&e);
        assert_eq!(event.source_key, "series-abc:2026-08-15T09:00:00-07:00");
    }

    #[test]
    fn extras_carry_the_calendar_event_kinds_own_fields() {
        let event = calendar_event_to_event(&evt());
        assert_eq!(event.extras.get("organizer"), Some(&FieldValue::Str("boss@example.com".to_string())));
        assert_eq!(
            event.extras.get("attendees"),
            Some(&FieldValue::StrList(vec!["boss@example.com".to_string(), "me@example.com".to_string()]))
        );
        assert_eq!(event.extras.get("is_all_day"), Some(&FieldValue::Bool(false)));
        assert_eq!(event.extras.get("response"), Some(&FieldValue::Str("accepted".to_string())));
    }

    #[test]
    fn the_same_event_parsed_twice_produces_a_byte_identical_result() {
        assert_eq!(calendar_event_to_event(&evt()), calendar_event_to_event(&evt()));
    }
}
