//! Mapping one [`MailMessage`] onto the ADR-0013 `email`-kind [`Event`]
//! shape the rule engine evaluates against (`hummingbird_domain::
//! EVENT_KINDS`) — `gmail_poll::event`'s own role, for Graph's shape.
//! Graph carries no Gmail-style `labelIds`; the `email` kind's `labels`
//! field is filled from the message's own `categories` instead — the
//! nearest Graph analog (operator-assigned tags on a message).

use std::collections::BTreeMap;

use hummingbird_domain::{m365_mail_v1_key, now_as_deadline, Event, FieldValue, M365_MAIL_V1};

use crate::evaluate::Candidate;
use crate::mail_message::MailMessage;

/// Builds the `email`-kind [`Event`] one Graph message presents to the
/// rule engine. Pure: no clock read beyond what `msg.received_at_ms`
/// already carries. `m365-mail/v1` never expires
/// (`hummingbird_domain::sources::REGISTRY`), so the returned
/// [`Candidate::ends_at_ms`] is always `None` — `alert.rs::plan` relays
/// that straight through to `expires_at`.
pub fn mail_message_to_candidate(msg: &MailMessage) -> Candidate {
    let title = if msg.subject.is_empty() { "(no subject)".to_string() } else { msg.subject.clone() };
    let received_at = now_as_deadline(msg.received_at_ms);

    let mut extras = BTreeMap::new();
    extras.insert("from".to_string(), FieldValue::Str(msg.from_address.clone().unwrap_or_default()));
    extras.insert("to".to_string(), FieldValue::StrList(msg.to_addresses.clone()));
    extras.insert("cc".to_string(), FieldValue::StrList(msg.cc_addresses.clone()));
    extras.insert("subject".to_string(), FieldValue::Str(msg.subject.clone()));
    extras.insert("labels".to_string(), FieldValue::StrList(msg.categories.clone()));
    extras.insert("received_at".to_string(), FieldValue::Str(received_at.clone()));
    extras.insert("has_attachment".to_string(), FieldValue::Bool(msg.has_attachments));

    let event = Event {
        source: M365_MAIL_V1.to_string(),
        source_key: m365_mail_v1_key(&msg.internet_message_id),
        occurred_at: received_at,
        title,
        body: (!msg.body_preview.is_empty()).then(|| msg.body_preview.clone()),
        url: msg.web_link.clone(),
        severity: None,
        calendar_busy: None,
        event_kind: Some("email".to_string()),
        extras,
    };
    Candidate { event, ends_at_ms: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg() -> MailMessage {
        MailMessage {
            internet_message_id: "<abc123@contoso.com>".to_string(),
            received_at_ms: 1_786_784_400_000,
            subject: "Q3 numbers".to_string(),
            body_preview: "please review".to_string(),
            from_address: Some("boss@contoso.com".to_string()),
            to_addresses: vec!["me@contoso.com".to_string()],
            cc_addresses: vec![],
            categories: vec!["Red category".to_string()],
            has_attachments: true,
            web_link: Some("https://outlook.office.com/mail/inbox/id/abc".to_string()),
        }
    }

    #[test]
    fn the_core_fields_are_populated_from_the_message() {
        let candidate = mail_message_to_candidate(&msg());
        assert_eq!(candidate.event.source, "m365-mail/v1");
        assert_eq!(candidate.event.source_key, "<abc123@contoso.com>");
        assert_eq!(candidate.event.title, "Q3 numbers");
        assert_eq!(candidate.event.body.as_deref(), Some("please review"));
        assert_eq!(candidate.event.event_kind.as_deref(), Some("email"));
        assert_eq!(candidate.ends_at_ms, None, "mail never expires");
    }

    #[test]
    fn a_missing_subject_falls_back_rather_than_an_empty_title() {
        let mut m = msg();
        m.subject = String::new();
        let candidate = mail_message_to_candidate(&m);
        assert_eq!(candidate.event.title, "(no subject)");
    }

    #[test]
    fn an_empty_body_preview_is_no_body_never_an_empty_string() {
        let mut m = msg();
        m.body_preview = String::new();
        let candidate = mail_message_to_candidate(&m);
        assert_eq!(candidate.event.body, None);
    }

    #[test]
    fn categories_populate_the_labels_field() {
        let candidate = mail_message_to_candidate(&msg());
        assert_eq!(
            candidate.event.extras.get("labels"),
            Some(&FieldValue::StrList(vec!["Red category".to_string()]))
        );
    }

    #[test]
    fn the_same_message_parsed_twice_produces_a_byte_identical_candidate() {
        assert_eq!(mail_message_to_candidate(&msg()), mail_message_to_candidate(&msg()));
    }
}
