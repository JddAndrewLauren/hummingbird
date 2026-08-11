//! Mapping one [`GmailMessage`] onto the ADR-0013 [`Event`] shape the rule
//! engine evaluates against — the `email` kind's own field table
//! (`hummingbird_domain::EVENT_KINDS`).

use std::collections::BTreeMap;

use hummingbird_domain::{gmail_v1_key, now_as_deadline, Event, FieldValue, GMAIL_V1};

use crate::message::GmailMessage;

/// Splits a header's comma-separated address list into trimmed, non-empty
/// entries — `to`/`cc` are declared `string_list` (the `email` kind's own
/// table), and Gmail's header value is one comma-joined string.
fn address_list(header: Option<&str>) -> Vec<String> {
    header
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Builds the `email`-kind [`Event`] one Gmail message presents to the rule
/// engine (ADR-0011: "hands the batch to the rule engine in memory"). Pure:
/// no clock read beyond what `msg.internal_date_ms` already carries.
pub fn message_to_event(msg: &GmailMessage) -> Event {
    let subject = msg.header("subject").unwrap_or("").to_string();
    let title = if subject.is_empty() { "(no subject)".to_string() } else { subject.clone() };
    let received_at = now_as_deadline(msg.internal_date_ms);

    let mut extras = BTreeMap::new();
    extras.insert("from".to_string(), FieldValue::Str(msg.header("from").unwrap_or("").to_string()));
    extras.insert("to".to_string(), FieldValue::StrList(address_list(msg.header("to"))));
    extras.insert("cc".to_string(), FieldValue::StrList(address_list(msg.header("cc"))));
    extras.insert("subject".to_string(), FieldValue::Str(subject));
    extras.insert("labels".to_string(), FieldValue::StrList(msg.label_ids.clone()));
    extras.insert("received_at".to_string(), FieldValue::Str(received_at.clone()));
    extras.insert("has_attachment".to_string(), FieldValue::Bool(msg.has_attachment));

    Event {
        source: GMAIL_V1.to_string(),
        source_key: gmail_v1_key(&msg.id),
        occurred_at: received_at,
        title,
        body: (!msg.snippet.is_empty()).then(|| msg.snippet.clone()),
        url: Some(format!("https://mail.google.com/mail/u/0/#all/{}", msg.id)),
        severity: None,
        calendar_busy: None,
        event_kind: Some("email".to_string()),
        extras,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap as Map;

    fn msg(headers: &[(&str, &str)], label_ids: &[&str], snippet: &str, attachment: bool) -> GmailMessage {
        let mut h = Map::new();
        for (k, v) in headers {
            h.insert(k.to_string(), v.to_string());
        }
        GmailMessage {
            id: "m-1".to_string(),
            internal_date_ms: 1_691_700_000_000,
            headers: h,
            label_ids: label_ids.iter().map(|s| s.to_string()).collect(),
            snippet: snippet.to_string(),
            has_attachment: attachment,
        }
    }

    #[test]
    fn the_core_fields_are_populated_from_the_message() {
        let m = msg(&[("subject", "Q3 numbers")], &["INBOX"], "preview text", false);
        let event = message_to_event(&m);
        assert_eq!(event.source, "gmail/v1");
        assert_eq!(event.source_key, "m-1", "the bare message id (#158's gmail_v1_key)");
        assert_eq!(event.title, "Q3 numbers");
        assert_eq!(event.body.as_deref(), Some("preview text"));
        assert_eq!(event.event_kind.as_deref(), Some("email"));
        assert_eq!(event.occurred_at, "2023-08-10T20:40");
    }

    #[test]
    fn a_missing_subject_falls_back_rather_than_an_empty_title() {
        let m = msg(&[], &[], "", false);
        let event = message_to_event(&m);
        assert_eq!(event.title, "(no subject)");
    }

    #[test]
    fn an_empty_snippet_is_no_body_never_an_empty_string() {
        let m = msg(&[], &[], "", false);
        let event = message_to_event(&m);
        assert_eq!(event.body, None);
    }

    #[test]
    fn to_and_cc_split_a_comma_joined_header_into_a_trimmed_list() {
        let m = msg(&[("to", "a@x.com, b@x.com ,c@x.com")], &[], "", false);
        let event = message_to_event(&m);
        assert_eq!(
            event.extras.get("to"),
            Some(&FieldValue::StrList(vec![
                "a@x.com".to_string(),
                "b@x.com".to_string(),
                "c@x.com".to_string()
            ]))
        );
    }

    #[test]
    fn extras_carry_labels_and_has_attachment() {
        let m = msg(&[], &["IMPORTANT", "CATEGORY_UPDATES"], "", true);
        let event = message_to_event(&m);
        assert_eq!(
            event.extras.get("labels"),
            Some(&FieldValue::StrList(vec!["IMPORTANT".to_string(), "CATEGORY_UPDATES".to_string()]))
        );
        assert_eq!(event.extras.get("has_attachment"), Some(&FieldValue::Bool(true)));
    }

    #[test]
    fn the_same_message_parsed_twice_produces_a_byte_identical_event() {
        let m = msg(&[("subject", "s")], &["INBOX"], "p", false);
        assert_eq!(message_to_event(&m), message_to_event(&m));
    }
}
