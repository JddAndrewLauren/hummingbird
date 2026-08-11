//! Mapping one [`CalendarItem`] onto the ADR-0013 `calendar_event`-kind
//! [`Event`] shape the rule engine evaluates against
//! (`hummingbird_domain::EVENT_KINDS`) — `calendar_poll::event`'s own role,
//! for Graph's shape.

use std::collections::BTreeMap;

use hummingbird_domain::{m365_calendar_v1_key, now_as_deadline, Event, FieldValue, M365_CALENDAR_V1};

use crate::calendar_item::CalendarItem;
use crate::evaluate::Candidate;

/// This poller polls exactly one calendar (the operator's own primary
/// mailbox calendar) — `calendar_poll::event`'s own `CALENDAR_ID` reasoning
/// applies verbatim.
const CALENDAR_ID: &str = "primary";

/// Builds the `calendar_event`-kind [`Event`]/[`Candidate`] one Graph event
/// presents to the rule engine. Pure: no clock read beyond what
/// `evt.starts_at_ms`/`evt.ends_at_ms` already carry. `m365-calendar/v1` is
/// registered `Expiry::Always("the instance's end time")`
/// (`hummingbird_domain::sources::REGISTRY`), so the returned
/// [`Candidate::ends_at_ms`] is always `Some` — `alert.rs::plan` relays
/// that straight through to `expires_at`.
pub fn calendar_item_to_candidate(evt: &CalendarItem) -> Candidate {
    let title = if evt.subject.is_empty() { "(untitled event)".to_string() } else { evt.subject.clone() };
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
    extras.insert("response".to_string(), FieldValue::Str(evt.response.clone().unwrap_or_default()));

    let event = Event {
        source: M365_CALENDAR_V1.to_string(),
        source_key: m365_calendar_v1_key(&evt.id, evt.series_master_id.as_deref(), &evt.id),
        occurred_at: starts_at,
        title,
        body: (!evt.body_preview.is_empty()).then(|| evt.body_preview.clone()),
        url: evt.web_link.clone(),
        severity: None,
        calendar_busy: None,
        event_kind: Some("calendar_event".to_string()),
        extras,
    };
    Candidate { event, ends_at_ms: Some(evt.ends_at_ms) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evt() -> CalendarItem {
        CalendarItem {
            id: "evt-1".to_string(),
            series_master_id: None,
            subject: "Board review".to_string(),
            body_preview: "quarterly numbers".to_string(),
            web_link: Some("https://outlook.office.com/calendar/item/abc".to_string()),
            organizer_email: Some("boss@contoso.com".to_string()),
            location: Some("Room 4".to_string()),
            response: Some("accepted".to_string()),
            attendee_emails: vec!["boss@contoso.com".to_string(), "me@contoso.com".to_string()],
            is_all_day: false,
            starts_at_ms: 1_786_871_400_000,
            ends_at_ms: 1_786_873_200_000,
        }
    }

    #[test]
    fn the_core_fields_are_populated_from_the_event() {
        let candidate = calendar_item_to_candidate(&evt());
        assert_eq!(candidate.event.source, "m365-calendar/v1");
        assert_eq!(candidate.event.source_key, "evt-1:evt-1");
        assert_eq!(candidate.event.title, "Board review");
        assert_eq!(candidate.event.body.as_deref(), Some("quarterly numbers"));
        assert_eq!(candidate.event.event_kind.as_deref(), Some("calendar_event"));
        assert_eq!(candidate.ends_at_ms, Some(1_786_873_200_000));
    }

    /// The key's second half uses `id` in place of a Graph-provided
    /// "original start" (this module's own doc: Graph carries no such
    /// field, but its occurrence `id` is documented stable across a
    /// reschedule) — so two occurrences of one series still key
    /// differently, purely on `id`, matching #158's own invariant test for
    /// the Google leg.
    #[test]
    fn a_recurring_instance_keys_on_the_series_id_and_its_own_occurrence_id() {
        let mut e = evt();
        e.id = "occ-1".to_string();
        e.series_master_id = Some("series-abc".to_string());
        let candidate = calendar_item_to_candidate(&e);
        assert_eq!(candidate.event.source_key, "series-abc:occ-1");
    }

    #[test]
    fn two_occurrences_of_one_series_key_differently() {
        let mut a = evt();
        a.id = "occ-1".to_string();
        a.series_master_id = Some("series-abc".to_string());
        let mut b = evt();
        b.id = "occ-2".to_string();
        b.series_master_id = Some("series-abc".to_string());
        assert_ne!(calendar_item_to_candidate(&a).event.source_key, calendar_item_to_candidate(&b).event.source_key);
    }

    /// The failure mode #137's own brief calls out: a rescheduled
    /// occurrence must still land on the row already minted for it. Graph
    /// keeps `id` stable across a reschedule (this module's documented
    /// assumption), so re-mapping the same occurrence after only its
    /// `start`/`end` changed must still produce the same key.
    #[test]
    fn a_rescheduled_occurrence_still_keys_the_same() {
        let mut before = evt();
        before.id = "occ-1".to_string();
        before.series_master_id = Some("series-abc".to_string());
        let mut after = before.clone();
        after.starts_at_ms += 3_600_000;
        after.ends_at_ms += 3_600_000;
        assert_eq!(calendar_item_to_candidate(&before).event.source_key, calendar_item_to_candidate(&after).event.source_key);
    }

    #[test]
    fn a_missing_subject_falls_back_rather_than_an_empty_title() {
        let mut e = evt();
        e.subject = String::new();
        let candidate = calendar_item_to_candidate(&e);
        assert_eq!(candidate.event.title, "(untitled event)");
    }

    #[test]
    fn extras_carry_the_calendar_event_kinds_own_fields() {
        let candidate = calendar_item_to_candidate(&evt());
        assert_eq!(candidate.event.extras.get("organizer"), Some(&FieldValue::Str("boss@contoso.com".to_string())));
        assert_eq!(candidate.event.extras.get("is_all_day"), Some(&FieldValue::Bool(false)));
        assert_eq!(candidate.event.extras.get("location"), Some(&FieldValue::Str("Room 4".to_string())));
        assert_eq!(candidate.event.extras.get("response"), Some(&FieldValue::Str("accepted".to_string())));
    }

    /// The failure this fix exists for (PR #277's review): `location`/
    /// `response` used to be inserted as invented empty strings for facts
    /// Graph actually carries. ADR-0013's missing-field rule (false even
    /// when negated) covers only an ABSENT field — a PRESENT empty string
    /// makes a negated condition like "NOT (location eq 'Room 4')" evaluate
    /// TRUE for an event that is in fact in Room 4, firing an alert on an
    /// invented value. With the real value parsed through, the negated
    /// condition correctly does not match.
    #[test]
    fn a_negated_location_condition_does_not_fire_on_an_event_actually_at_that_location() {
        use hummingbird_domain::{Condition, Rule, Tier};
        use hummingbird_rules_engine::{evaluate_rules, RuleOutcome};

        let rule = Rule {
            id: "r-1".to_string(),
            name: "not room 4".to_string(),
            event_kind: Some("calendar_event".to_string()),
            conditions: vec![Condition {
                field: "location".to_string(),
                op: "eq".to_string(),
                value: serde_json::json!("Room 4"),
                negate: true,
            }],
            severity: "high".to_string(),
            tier: Tier::Normal,
            enabled: true,
            updated_at: 0,
            version: 1,
        };
        let candidate = calendar_item_to_candidate(&evt()); // location: "Room 4"
        let outcomes = evaluate_rules(&[rule], &candidate.event, "2026-08-15T09:00");
        assert!(
            outcomes.iter().all(|(_, o)| !matches!(o, RuleOutcome::Matched(_))),
            "an event actually in Room 4 must not fire NOT(location eq 'Room 4')"
        );
    }

    #[test]
    fn the_same_event_parsed_twice_produces_a_byte_identical_result() {
        assert_eq!(calendar_item_to_candidate(&evt()), calendar_item_to_candidate(&evt()));
    }
}
