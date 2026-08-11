//! Parsing one Gmail `messages.get` (`format=full`) response into a typed
//! [`GmailMessage`]. Everything downstream (`event.rs`) reads only this
//! type, never the raw JSON, so the one place that has to know Gmail's
//! shape is this module.

use std::collections::BTreeMap;
use std::fmt;

/// Why a message response could not be read — named rather than a quietly
/// skipped message, the same "malformed with a reason" discipline
/// `hummingbird_domain::EnvelopeProblem` uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageError {
    NotJson(String),
    MissingField(&'static str),
    WrongType(&'static str),
}

impl fmt::Display for MessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageError::NotJson(m) => write!(f, "message response is not JSON: {m}"),
            MessageError::MissingField(field) => write!(f, "message is missing `{field}`"),
            MessageError::WrongType(field) => write!(f, "message's `{field}` is the wrong type"),
        }
    }
}

/// The Gmail fields `event.rs` needs, decoded once. `headers` is keyed
/// lower-case (Gmail's own casing, e.g. `From`, varies by message and by
/// client) so a lookup never has to guess a header's original case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GmailMessage {
    pub id: String,
    /// Epoch ms, Gmail's own `internalDate` — the receive time the mail
    /// server stamped, not a header a sender could forge.
    pub internal_date_ms: i64,
    pub headers: BTreeMap<String, String>,
    pub label_ids: Vec<String>,
    /// Gmail's own decoded, truncated plain-text preview — used as the
    /// event's `body` rather than decoding the full MIME tree, which no
    /// rule condition or alert display needs.
    pub snippet: String,
    pub has_attachment: bool,
}

impl GmailMessage {
    /// A header by lower-cased name, e.g. `header("from")`.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }
}

/// Parses one `messages.get(format=full)` response body.
pub fn parse_message(json: &str) -> Result<GmailMessage, MessageError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| MessageError::NotJson(e.to_string()))?;
    let object = value.as_object().ok_or(MessageError::WrongType("<root>"))?;

    let id = text_field(object, "id")?;

    let internal_date_ms = object
        .get("internalDate")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .ok_or(MessageError::MissingField("internalDate"))?;

    let label_ids = object
        .get("labelIds")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();

    let snippet = object.get("snippet").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let payload = object.get("payload");
    let headers = payload
        .and_then(|p| p.get("headers"))
        .and_then(|h| h.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let name = entry.get("name")?.as_str()?.to_ascii_lowercase();
                    let value = entry.get("value")?.as_str()?.to_string();
                    Some((name, value))
                })
                .collect()
        })
        .unwrap_or_default();

    let has_attachment = payload.map(part_has_attachment).unwrap_or(false);

    Ok(GmailMessage {
        id,
        internal_date_ms,
        headers,
        label_ids,
        snippet,
        has_attachment,
    })
}

/// A part (or the top-level `payload`, which has the same shape) carries an
/// attachment if it names a non-empty `filename` itself, or any of its
/// nested `parts` does — MIME multipart nests arbitrarily (an attachment
/// inside a `multipart/mixed` inside a `multipart/alternative`, etc.).
fn part_has_attachment(part: &serde_json::Value) -> bool {
    let named = part
        .get("filename")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty());
    if named {
        return true;
    }
    part.get("parts")
        .and_then(|v| v.as_array())
        .is_some_and(|parts| parts.iter().any(part_has_attachment))
}

fn text_field(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &'static str,
) -> Result<String, MessageError> {
    match object.get(field) {
        Some(serde_json::Value::String(s)) if !s.is_empty() => Ok(s.clone()),
        Some(serde_json::Value::String(_)) => Err(MessageError::WrongType(field)),
        Some(_) => Err(MessageError::WrongType(field)),
        None => Err(MessageError::MissingField(field)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(headers: &str, parts: &str) -> String {
        format!(
            r#"{{"id": "18c2f9a0b1e2d3f4", "threadId": "t-1", "labelIds": ["INBOX", "UNREAD"],
                "snippet": "hello there", "internalDate": "1691700000000",
                "payload": {{"mimeType": "multipart/mixed", "headers": [{headers}], "parts": [{parts}]}}}}"#
        )
    }

    #[test]
    fn a_full_message_parses_every_field() {
        let json = sample(
            r#"{"name": "From", "value": "boss@example.com"},
               {"name": "Subject", "value": "Q3 numbers"}"#,
            r#"{"filename": "", "parts": [{"filename": "report.pdf"}]}"#,
        );
        let msg = parse_message(&json).expect("well-formed message parses");
        assert_eq!(msg.id, "18c2f9a0b1e2d3f4");
        assert_eq!(msg.internal_date_ms, 1_691_700_000_000);
        assert_eq!(msg.header("from"), Some("boss@example.com"));
        assert_eq!(msg.header("subject"), Some("Q3 numbers"));
        assert_eq!(msg.label_ids, vec!["INBOX", "UNREAD"]);
        assert_eq!(msg.snippet, "hello there");
        assert!(msg.has_attachment, "nested part names a filename");
    }

    #[test]
    fn header_lookup_is_case_insensitive_to_gmails_own_casing() {
        let json = sample(r#"{"name": "SUBJECT", "value": "shout"}"#, "");
        let msg = parse_message(&json).unwrap();
        assert_eq!(msg.header("subject"), Some("shout"));
    }

    #[test]
    fn no_named_part_is_no_attachment() {
        let json = sample("", r#"{"filename": ""}"#);
        let msg = parse_message(&json).unwrap();
        assert!(!msg.has_attachment);
    }

    #[test]
    fn a_message_with_no_payload_at_all_still_parses_with_no_headers_or_attachment() {
        let json = r#"{"id": "m-1", "internalDate": "1000", "labelIds": [], "snippet": ""}"#;
        let msg = parse_message(json).expect("a payload-less message is not malformed");
        assert!(msg.headers.is_empty());
        assert!(!msg.has_attachment);
    }

    #[test]
    fn missing_id_or_internal_date_is_named() {
        assert_eq!(
            parse_message(r#"{"internalDate": "1"}"#),
            Err(MessageError::MissingField("id"))
        );
        assert_eq!(
            parse_message(r#"{"id": "m-1"}"#),
            Err(MessageError::MissingField("internalDate"))
        );
    }

    #[test]
    fn not_json_is_named() {
        assert!(matches!(parse_message("not json"), Err(MessageError::NotJson(_))));
    }
}
