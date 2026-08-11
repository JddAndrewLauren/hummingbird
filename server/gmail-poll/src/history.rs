//! Parsing Gmail's `history.list` and `getProfile` responses — the delta
//! half of ADR-0011's "per-source delta cursor". `history.list` answers
//! *changes since a `historyId`*; Gmail itself decides when that id has
//! aged out (an HTTP 404, `main.rs`'s job to notice, never this module's —
//! this module only ever sees a 200 body).

use std::fmt;

/// Why a `history.list`/`getProfile` body could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryError {
    NotJson(String),
    MissingField(&'static str),
}

impl fmt::Display for HistoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HistoryError::NotJson(m) => write!(f, "response is not JSON: {m}"),
            HistoryError::MissingField(field) => write!(f, "response is missing `{field}`"),
        }
    }
}

/// One `history.list` page: the message ids Gmail reports as *added* since
/// the cursor (edits and deletions are not evaluated — a rule fires against
/// mail arriving, ADR-0013's `email` kind names no "edited" or "deleted"
/// shape), plus the token for the next page and the mailbox's current
/// `historyId` as of this response, when Gmail includes it (present on
/// every real response; `None` only for a hand-built fixture missing it).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HistoryPage {
    pub message_ids: Vec<String>,
    pub next_page_token: Option<String>,
    pub history_id: Option<String>,
}

/// Parses one `users.history.list` response body. A response with no
/// `history` key at all is a legitimate "nothing changed" page, not a
/// malformed one — Gmail omits the key entirely rather than sending `[]`.
pub fn parse_history_list(json: &str) -> Result<HistoryPage, HistoryError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| HistoryError::NotJson(e.to_string()))?;
    let object = value.as_object().ok_or_else(|| HistoryError::NotJson("not an object".into()))?;

    let message_ids = object
        .get("history")
        .and_then(|v| v.as_array())
        .map(|entries| {
            entries
                .iter()
                .flat_map(|entry| {
                    entry
                        .get("messagesAdded")
                        .and_then(|v| v.as_array())
                        .into_iter()
                        .flatten()
                        .filter_map(|added| added.get("message")?.get("id")?.as_str())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();

    let next_page_token =
        object.get("nextPageToken").and_then(|v| v.as_str()).map(str::to_string);
    let history_id = object.get("historyId").and_then(|v| v.as_str()).map(str::to_string);

    Ok(HistoryPage { message_ids, next_page_token, history_id })
}

/// Parses one `users.getProfile` response body, returning the mailbox's
/// current `historyId` — the bounded re-sync's own "where to resume from",
/// since a re-sync by definition has no earlier cursor to read one off of.
pub fn parse_profile(json: &str) -> Result<String, HistoryError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| HistoryError::NotJson(e.to_string()))?;
    value
        .get("historyId")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or(HistoryError::MissingField("historyId"))
}

/// One `users.messages.list` response body — the bounded re-sync's message
/// discovery (no `history` semantics at all, just "what's here now").
pub fn parse_messages_list(json: &str) -> Result<Vec<String>, HistoryError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| HistoryError::NotJson(e.to_string()))?;
    let ids = value
        .get("messages")
        .and_then(|v| v.as_array())
        .map(|entries| entries.iter().filter_map(|m| m.get("id")?.as_str()).map(str::to_string).collect())
        .unwrap_or_default();
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_with_messages_added_collects_their_ids() {
        let json = r#"{
            "history": [
                {"id": "100", "messagesAdded": [{"message": {"id": "m-1", "threadId": "t-1"}}]},
                {"id": "101", "messagesAdded": [{"message": {"id": "m-2", "threadId": "t-2"}}]}
            ],
            "historyId": "102"
        }"#;
        let page = parse_history_list(json).unwrap();
        assert_eq!(page.message_ids, vec!["m-1", "m-2"]);
        assert_eq!(page.history_id.as_deref(), Some("102"));
        assert_eq!(page.next_page_token, None);
    }

    /// Gmail's `history` entries carry other change kinds too
    /// (`labelsAdded`, `messagesDeleted`, …) — only `messagesAdded` is
    /// evaluated, since a rule fires against mail arriving.
    #[test]
    fn only_messages_added_is_read_other_history_kinds_are_ignored() {
        let json = r#"{
            "history": [
                {"id": "100", "labelsAdded": [{"message": {"id": "m-1"}, "labelIds": ["UNREAD"]}]},
                {"id": "101", "messagesDeleted": [{"message": {"id": "m-2"}}]},
                {"id": "102", "messagesAdded": [{"message": {"id": "m-3"}}]}
            ]
        }"#;
        let page = parse_history_list(json).unwrap();
        assert_eq!(page.message_ids, vec!["m-3"]);
    }

    #[test]
    fn a_response_with_no_history_key_is_an_empty_page_not_malformed() {
        let json = r#"{"historyId": "50"}"#;
        let page = parse_history_list(json).expect("nothing changed is not an error");
        assert!(page.message_ids.is_empty());
        assert_eq!(page.history_id.as_deref(), Some("50"));
    }

    #[test]
    fn a_next_page_token_is_carried_through() {
        let json = r#"{"history": [], "nextPageToken": "tok-abc", "historyId": "50"}"#;
        let page = parse_history_list(json).unwrap();
        assert_eq!(page.next_page_token.as_deref(), Some("tok-abc"));
    }

    #[test]
    fn not_json_is_named() {
        assert!(matches!(parse_history_list("nope"), Err(HistoryError::NotJson(_))));
    }

    #[test]
    fn profile_reads_the_history_id() {
        let json = r#"{"emailAddress": "me@example.com", "messagesTotal": 5, "historyId": "999"}"#;
        assert_eq!(parse_profile(json), Ok("999".to_string()));
    }

    #[test]
    fn profile_with_no_history_id_is_a_named_error() {
        let json = r#"{"emailAddress": "me@example.com"}"#;
        assert_eq!(parse_profile(json), Err(HistoryError::MissingField("historyId")));
    }

    #[test]
    fn messages_list_collects_ids() {
        let json = r#"{"messages": [{"id": "m-1", "threadId": "t-1"}, {"id": "m-2", "threadId": "t-2"}], "resultSizeEstimate": 2}"#;
        assert_eq!(parse_messages_list(json).unwrap(), vec!["m-1", "m-2"]);
    }

    #[test]
    fn messages_list_with_no_results_is_an_empty_list() {
        let json = r#"{"resultSizeEstimate": 0}"#;
        assert_eq!(parse_messages_list(json).unwrap(), Vec::<String>::new());
    }
}
