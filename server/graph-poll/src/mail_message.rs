//! Parsing one Microsoft Graph `message` resource (a delta-query item on
//! `mailFolders('inbox')/messages/delta`) into a typed [`MailMessage`].
//! Everything downstream (`mail_event.rs`) reads only this type, never the
//! raw JSON — `gmail_poll::message`'s own split, for Graph's shape.

use std::fmt;

/// Why a message body could not be read — named rather than a quietly
/// skipped message, the same "malformed with a reason" discipline
/// `gmail_poll::MessageError`/`calendar_poll::CalendarEventError` use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailMessageError {
    NotJson(String),
    MissingField(&'static str),
    BadTimestamp(&'static str),
}

impl fmt::Display for MailMessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MailMessageError::NotJson(m) => write!(f, "message response is not JSON: {m}"),
            MailMessageError::MissingField(field) => write!(f, "message is missing `{field}`"),
            MailMessageError::BadTimestamp(field) => write!(f, "message's `{field}` is not a readable date/time"),
        }
    }
}

/// The Graph message fields `mail_event.rs` needs, decoded once.
#[derive(Debug, Clone, PartialEq)]
pub struct MailMessage {
    /// `internetMessageId`, never the Graph `id` — #158's own key recipe
    /// (`hummingbird_domain::m365_mail_v1_key`'s doc: "never the Graph
    /// `id`, which changes on a folder move").
    pub internet_message_id: String,
    pub received_at_ms: i64,
    pub subject: String,
    pub body_preview: String,
    pub from_address: Option<String>,
    pub to_addresses: Vec<String>,
    pub cc_addresses: Vec<String>,
    pub categories: Vec<String>,
    pub has_attachments: bool,
    pub web_link: Option<String>,
}

/// The result of parsing one delta-query item: a live message, or Graph's
/// own "this item left the collection" marker (`"@removed": {"reason":
/// "..."}`, present uniformly across every Graph delta resource type —
/// `delta.rs`'s own module doc). [`ParsedMailMessage::Removed`] is named
/// separately from [`MailMessageError`] because it is not a parse failure:
/// it is the expected shape for a deletion or a move out of the polled
/// folder, and must be skipped loudly but non-fatally rather than logged
/// as malformed.
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedMailMessage {
    Live(Box<MailMessage>),
    Removed,
}

/// Parses one delta-query message item body.
pub fn parse_mail_message(json: &str) -> Result<ParsedMailMessage, MailMessageError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| MailMessageError::NotJson(e.to_string()))?;
    let object = value.as_object().ok_or_else(|| MailMessageError::NotJson("not an object".into()))?;

    if object.contains_key("@removed") {
        return Ok(ParsedMailMessage::Removed);
    }

    let internet_message_id =
        object.get("internetMessageId").and_then(|v| v.as_str()).map(str::to_string).ok_or(MailMessageError::MissingField("internetMessageId"))?;
    let received_at_ms = object
        .get("receivedDateTime")
        .and_then(|v| v.as_str())
        .ok_or(MailMessageError::MissingField("receivedDateTime"))
        .and_then(|s| s.parse::<jiff::Timestamp>().map_err(|_| MailMessageError::BadTimestamp("receivedDateTime")))?
        .as_millisecond();

    let subject = object.get("subject").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let body_preview = object.get("bodyPreview").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let from_address = object
        .get("from")
        .and_then(|f| f.get("emailAddress"))
        .and_then(|e| e.get("address"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let to_addresses = recipient_addresses(object.get("toRecipients"));
    let cc_addresses = recipient_addresses(object.get("ccRecipients"));
    let categories = object
        .get("categories")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).map(str::to_string).collect())
        .unwrap_or_default();
    let has_attachments = object.get("hasAttachments").and_then(|v| v.as_bool()).unwrap_or(false);
    let web_link = object.get("webLink").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(str::to_string);

    Ok(ParsedMailMessage::Live(Box::new(MailMessage {
        internet_message_id,
        received_at_ms,
        subject,
        body_preview,
        from_address,
        to_addresses,
        cc_addresses,
        categories,
        has_attachments,
        web_link,
    })))
}

fn recipient_addresses(v: Option<&serde_json::Value>) -> Vec<String> {
    v.and_then(|v| v.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|e| e.get("emailAddress")?.get("address")?.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message_json(internet_message_id: &str, extra: &str) -> String {
        format!(
            r#"{{
                "id": "AAMkAGI1AAA=-inbox",
                "internetMessageId": "{internet_message_id}",
                "subject": "Q3 numbers",
                "bodyPreview": "please review",
                "receivedDateTime": "2026-08-15T09:00:00Z",
                "from": {{"emailAddress": {{"address": "boss@contoso.com"}}}},
                "webLink": "https://outlook.office.com/mail/inbox/id/abc"
                {extra}
            }}"#
        )
    }

    #[test]
    fn a_live_message_parses_every_core_field() {
        let json = message_json("<abc123@contoso.com>", "");
        let parsed = parse_mail_message(&json).expect("well-formed message parses");
        let ParsedMailMessage::Live(msg) = parsed else { panic!("expected Live") };
        assert_eq!(msg.internet_message_id, "<abc123@contoso.com>");
        assert_eq!(msg.subject, "Q3 numbers");
        assert_eq!(msg.body_preview, "please review");
        assert_eq!(msg.from_address.as_deref(), Some("boss@contoso.com"));
        assert_eq!(msg.received_at_ms, 1_786_784_400_000);
    }

    /// Pins that the key input is `internetMessageId`, unaffected by the
    /// Graph `id` — modeled on `gmail_poll`'s own folder-move pin, for
    /// #158's stated reason.
    #[test]
    fn the_graph_id_is_never_read_as_the_key() {
        let json = message_json("<stable@contoso.com>", "");
        let parsed = parse_mail_message(&json).unwrap();
        let ParsedMailMessage::Live(msg) = parsed else { panic!() };
        assert_eq!(msg.internet_message_id, "<stable@contoso.com>");
    }

    #[test]
    fn a_removed_item_is_named_not_parsed_as_a_full_message() {
        let json = r#"{"id": "AAMk...", "@removed": {"reason": "deleted"}}"#;
        assert_eq!(parse_mail_message(json), Ok(ParsedMailMessage::Removed));
    }

    #[test]
    fn to_and_cc_addresses_are_read_from_the_nested_email_address() {
        let json = message_json(
            "<x@contoso.com>",
            r#", "toRecipients": [{"emailAddress": {"address": "me@contoso.com"}}],
                "ccRecipients": [{"emailAddress": {"address": "cc@contoso.com"}}]"#,
        );
        let ParsedMailMessage::Live(msg) = parse_mail_message(&json).unwrap() else { panic!() };
        assert_eq!(msg.to_addresses, vec!["me@contoso.com".to_string()]);
        assert_eq!(msg.cc_addresses, vec!["cc@contoso.com".to_string()]);
    }

    #[test]
    fn categories_and_has_attachments_are_read() {
        let json = message_json("<x@contoso.com>", r#", "categories": ["Red category"], "hasAttachments": true"#);
        let ParsedMailMessage::Live(msg) = parse_mail_message(&json).unwrap() else { panic!() };
        assert_eq!(msg.categories, vec!["Red category".to_string()]);
        assert!(msg.has_attachments);
    }

    #[test]
    fn missing_internet_message_id_is_named() {
        let json = r#"{"id": "x", "receivedDateTime": "2026-08-15T09:00:00Z"}"#;
        assert_eq!(parse_mail_message(json), Err(MailMessageError::MissingField("internetMessageId")));
    }

    #[test]
    fn missing_received_date_time_is_named() {
        let json = r#"{"id": "x", "internetMessageId": "<a@b.com>"}"#;
        assert_eq!(parse_mail_message(json), Err(MailMessageError::MissingField("receivedDateTime")));
    }

    #[test]
    fn not_json_is_named() {
        assert!(matches!(parse_mail_message("not json"), Err(MailMessageError::NotJson(_))));
    }
}
