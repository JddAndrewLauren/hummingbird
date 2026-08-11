//! Reading the address's page URL out of `GET /api/settings/:key`.
//!
//! Small, but it lives here rather than in `main.rs` for the reason that put
//! `fcm.rs` in `authority` instead of `worker`: it is a *decision* with real
//! failure modes, and `main.rs` has no test harness, so anything expressed
//! there is untested by construction. What `main.rs` keeps is the HTTP call
//! and the 404.
//!
//! The decode is two layers, which is the part worth pinning. `Setting.value`
//! stores the setting's **canonical JSON text**, so a URL arrives as a JSON
//! string *inside* the response's JSON string — `{"value": "\"https://…\""}`.
//! Reading `value` as a plain string yields `"https://…"` complete with
//! quotes, which is a perfectly valid URL-shaped thing that fetches nothing.

use std::fmt;

/// The binding key, resolved by name at the seam exactly as
/// `client/core/src/bindings.rs` does — unversioned, so a
/// `city-waste/v1 → /v2` source bump cannot orphan it.
pub const BINDING_KEY: &str = "city-waste-page";

/// Why a binding could not be turned into a URL. Never merged with "the row
/// does not exist" — that is a legitimate state (nobody has set the address
/// yet) and the caller exits quietly on it, where every arm below is a real
/// problem that deserves a log line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingProblem {
    /// The response, or the `value` inside it, was not JSON.
    NotJson,
    /// The row holds something that is not text — the same distinction
    /// `client/core/src/bindings.rs` draws with its `Other` value state, and
    /// `waste.ts` renders as `bound-but-unacquired`.
    NotText,
    /// Set, and set to nothing. Refused rather than fetched: `settings` has
    /// no DELETE, so a blanked row is how a binding gets un-set, and treating
    /// it as a URL would mean fetching the empty string.
    Empty,
}

impl fmt::Display for BindingProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BindingProblem::NotJson => write!(f, "`{BINDING_KEY}` is not readable JSON"),
            BindingProblem::NotText => write!(f, "`{BINDING_KEY}` does not hold text"),
            BindingProblem::Empty => write!(f, "`{BINDING_KEY}` is set to an empty value"),
        }
    }
}

/// Reads the page URL out of a `GET /api/settings/:key` response body.
pub fn page_url_from_response(body: &str) -> Result<String, BindingProblem> {
    let response: serde_json::Value =
        serde_json::from_str(body).map_err(|_| BindingProblem::NotJson)?;
    let stored = response.get("value").and_then(serde_json::Value::as_str).ok_or(
        // A response with no `value` at all is malformed rather than
        // untyped, but the caller's remedy is identical and inventing a
        // fourth arm to say so would be noise.
        BindingProblem::NotJson,
    )?;
    match serde_json::from_str::<serde_json::Value>(stored) {
        Ok(serde_json::Value::String(url)) if !url.is_empty() => Ok(url),
        Ok(serde_json::Value::String(_)) => Err(BindingProblem::Empty),
        Ok(_) => Err(BindingProblem::NotText),
        Err(_) => Err(BindingProblem::NotJson),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The double decode, spelled out on a real response shape. The inner
    /// value is quoted because `settings` stores canonical JSON text.
    #[test]
    fn a_url_binding_is_decoded_through_both_layers() {
        let body = r#"{"key":"city-waste-page","value":"\"https://city.example/collect?a=1\"","updated_at":1,"version":3}"#;
        assert_eq!(
            page_url_from_response(body),
            Ok("https://city.example/collect?a=1".to_string()),
            "the stored value's own quotes must not survive into the URL"
        );
    }

    /// A row holding a number, an object or a boolean is bound-but-unusable,
    /// not a URL — the distinction `bindings.rs` draws and the pane renders.
    #[test]
    fn a_row_that_is_not_text_is_refused_rather_than_stringified() {
        for stored in ["42", "true", r#"{"url":"https://x"}"#, "null", "[]"] {
            let body = format!(r#"{{"key":"city-waste-page","value":{}}}"#, serde_json::json!(stored));
            assert_eq!(page_url_from_response(&body), Err(BindingProblem::NotText), "{stored}");
        }
    }

    /// `settings` has no DELETE, so blanking the row is how a binding is
    /// un-set. Fetching the empty string is not a reading of that.
    #[test]
    fn a_blanked_row_is_refused_not_fetched() {
        let body = r#"{"key":"city-waste-page","value":"\"\""}"#;
        assert_eq!(page_url_from_response(body), Err(BindingProblem::Empty));
    }

    #[test]
    fn an_unreadable_response_is_named_rather_than_guessed_at() {
        for body in ["", "not json", "{}", r#"{"value":"not-json-inside"}"#] {
            assert_eq!(page_url_from_response(body), Err(BindingProblem::NotJson), "{body:?}");
        }
    }
}
