//! Which series to poll, read out of `GET /api/settings/race-series`.
//!
//! It lives here rather than in either `main.rs` for the reason that put
//! `fcm.rs` in `authority` instead of `worker`: it is a *decision* with real
//! failure modes, and `main.rs` has no test harness, so anything expressed
//! there is untested by construction. What each `main.rs` keeps is the HTTP
//! call and the 404.
//!
//! Both binaries read it. The route exists for precisely this — a poller
//! needs the binding that says *what* to poll, and duplicating the same fact
//! into an Actions secret would make the binding editor decorative.
//!
//! **The value is comma-separated series keys inside the JSON string** —
//! `"f1"` today, `"f1,indycar"` later. That is forced, not chosen: `settings`
//! stores each value's canonical JSON *text*, so a URL or a list arrives as a
//! JSON string inside the response's JSON string, and the client's
//! `BindingValue` vocabulary is `Unset | Text | Other` — a JSON **array**
//! would land as `Other`, which the shipped binding editor cannot write and
//! every pane reads as `bound-but-unacquired`.

use std::fmt;

/// The binding key, resolved by name at the seam exactly as
/// `client/core/src/bindings.rs` does — unversioned, so a
/// `race-schedule/v1 → /v2` source bump cannot orphan it.
pub const BINDING_KEY: &str = "race-series";

/// Every series key this build has an adapter for.
///
/// `indycar` is deliberately absent (#266): there is no free IndyCar JSON
/// API of comparable standing and no authoritative ICS, so that adapter is
/// its own issue. A series named in the binding and missing here is
/// **skipped and logged**, never an error — which is what makes the
/// deferral honest rather than silent, since the pane then renders the
/// series as a gap ("never polled") rather than quietly dropping it.
pub const ADAPTED_SERIES: &[&str] = &["f1"];

/// Whether this build can actually poll a series named in the binding.
pub fn has_adapter(series: &str) -> bool {
    ADAPTED_SERIES.contains(&series)
}

/// Why a binding could not be turned into a series list. Never merged with
/// "the row does not exist" — that is a legitimate state (nobody has chosen
/// a series yet) and the caller exits **0** on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingProblem {
    /// The response, or the `value` inside it, was not JSON. A malformed
    /// answer from the authority is a real failure, not a configuration
    /// state — the caller exits non-zero.
    NotJson,
    /// The row holds something that is not text — the same distinction
    /// `client/core/src/bindings.rs` draws with its `Other` value state.
    /// **Not configured**, not broken (see [`BindingProblem::is_unconfigured`]).
    NotText,
    /// Set, and set to nothing (or to nothing but separators). `settings`
    /// has no DELETE, so a blanked row is how a binding gets un-set.
    Empty,
}

impl BindingProblem {
    /// Whether this is "nobody has configured this yet" rather than "the
    /// lane is broken". The two must not share an exit code: a dead feed
    /// has to be loud (Actions emails the failed run) while an unconfigured
    /// one must be silent, exactly as an off-season season is.
    pub fn is_unconfigured(&self) -> bool {
        matches!(self, BindingProblem::NotText | BindingProblem::Empty)
    }
}

impl fmt::Display for BindingProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BindingProblem::NotJson => write!(f, "`{BINDING_KEY}` is not readable JSON"),
            BindingProblem::NotText => write!(f, "`{BINDING_KEY}` does not hold text"),
            BindingProblem::Empty => write!(f, "`{BINDING_KEY}` names no series"),
        }
    }
}

/// Reads the series list out of a `GET /api/settings/:key` response body.
///
/// Keys are trimmed and lowercased, blanks dropped, order preserved, and a
/// repeat is dropped rather than polled twice.
pub fn series_from_response(body: &str) -> Result<Vec<String>, BindingProblem> {
    let response: serde_json::Value =
        serde_json::from_str(body).map_err(|_| BindingProblem::NotJson)?;
    // A response with no `value` at all is malformed rather than untyped,
    // but the caller's remedy is identical and a fourth arm to say so would
    // be noise.
    let stored = response
        .get("value")
        .and_then(serde_json::Value::as_str)
        .ok_or(BindingProblem::NotJson)?;
    let text = match serde_json::from_str::<serde_json::Value>(stored) {
        Ok(serde_json::Value::String(text)) => text,
        Ok(_) => return Err(BindingProblem::NotText),
        Err(_) => return Err(BindingProblem::NotJson),
    };
    let mut series: Vec<String> = Vec::new();
    for key in text.split(',') {
        let key = key.trim().to_ascii_lowercase();
        if key.is_empty() || series.contains(&key) {
            continue;
        }
        series.push(key);
    }
    if series.is_empty() {
        return Err(BindingProblem::Empty);
    }
    Ok(series)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The double decode, spelled out on a real response shape. The inner
    /// value is quoted because `settings` stores canonical JSON text —
    /// reading `value` as a plain string yields `"f1"` complete with quotes,
    /// which is a perfectly plausible series key that matches no adapter.
    #[test]
    fn one_series_is_decoded_through_both_layers() {
        let body = r#"{"key":"race-series","value":"\"f1\"","updated_at":1,"version":3}"#;
        assert_eq!(series_from_response(body), Ok(vec!["f1".to_string()]));
    }

    /// The list is a comma-separated string, not a JSON array — a list as
    /// an array would land as the client's `Other` value state and be
    /// unwritable through the shipped binding editor.
    #[test]
    fn a_comma_separated_list_is_read_as_several_series() {
        let body = r#"{"key":"race-series","value":"\" F1 , indycar ,,f1 \""}"#;
        assert_eq!(
            series_from_response(body),
            Ok(vec!["f1".to_string(), "indycar".to_string()]),
            "trimmed, lowercased, blanks dropped, repeats dropped, order kept"
        );
    }

    /// A row holding an array, a number or an object is bound-but-unusable —
    /// and that is a *configuration* state, so the caller exits 0 and writes
    /// nothing rather than reporting the lane broken.
    #[test]
    fn a_row_that_is_not_text_is_unconfigured_rather_than_broken() {
        for stored in ["42", "true", r#"["f1"]"#, "null", r#"{"series":"f1"}"#] {
            let body = format!(
                r#"{{"key":"race-series","value":{}}}"#,
                serde_json::json!(stored)
            );
            let problem = series_from_response(&body).expect_err(stored);
            assert_eq!(problem, BindingProblem::NotText, "{stored}");
            assert!(problem.is_unconfigured(), "{stored}");
        }
    }

    /// `settings` has no DELETE, so blanking the row is how the binding is
    /// un-set — as is a row holding nothing but separators.
    #[test]
    fn a_blanked_row_names_no_series_and_is_unconfigured() {
        for stored in [r#""""#, r#""  ""#, r#"" , , ""#] {
            let body = format!(r#"{{"key":"race-series","value":{}}}"#, serde_json::json!(stored));
            let problem = series_from_response(&body).expect_err(stored);
            assert_eq!(problem, BindingProblem::Empty, "{stored}");
            assert!(problem.is_unconfigured());
        }
    }

    /// A malformed answer from the authority is the one arm that is NOT a
    /// configuration state: nothing about it says the operator has not set
    /// the binding, so it stays loud.
    #[test]
    fn an_unreadable_response_is_loud_rather_than_read_as_unconfigured() {
        for body in ["", "not json", "{}", r#"{"value":"not-json-inside"}"#] {
            let problem = series_from_response(body).expect_err(body);
            assert_eq!(problem, BindingProblem::NotJson, "{body:?}");
            assert!(!problem.is_unconfigured(), "{body:?}");
        }
    }

    /// The IndyCar deferral, as a fact the code carries rather than a note
    /// in an issue: a series in the binding with no adapter is recognised as
    /// such, so the caller can skip and log it instead of failing the run.
    #[test]
    fn indycar_is_named_in_the_binding_but_has_no_adapter_here() {
        assert!(has_adapter("f1"));
        assert!(!has_adapter("indycar"));
        assert!(!has_adapter("F1"), "series keys are lowercased on read");
    }
}
