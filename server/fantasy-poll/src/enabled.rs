//! Whether the Fantasy question is asked at all, read out of
//! `GET /api/settings/question-enabled-fantasy` (#715, ADR-0034).
//!
//! **Off means unpolled** (ADR-0034 decision 1): the binary reads this
//! first, and when it says off nothing else runs — no binding read, no
//! Yahoo call, nothing written. [`crate::gate`] repeats the check so that
//! "switched off" is also a no-fetch answer of the one pure gate.
//!
//! The reading is the core's, shared through `hummingbird_domain` rather
//! than re-stated: **absence is enabled** (a 404 — nobody has ever switched
//! it), and only the stored JSON literal `false` is off. A stored value
//! this build cannot interpret reads as **on**, because it must not silence
//! a question. A response from the authority that is not readable at all
//! is different: nothing about it says the operator switched anything, so
//! it is a [`SwitchProblem`] and stays loud.

use std::fmt;

/// The `settings.key` the switch is stored under — the core's own frozen
/// spelling, never a twin.
pub const SWITCH_KEY: &str = hummingbird_domain::FANTASY_SWITCH_KEY;

/// The authority's answer could not be read as a `settings` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchProblem;

impl fmt::Display for SwitchProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{SWITCH_KEY}` is not a readable settings row")
    }
}

/// Whether the question is on, given the body of
/// `GET /api/settings/question-enabled-fantasy` — or `None` when that GET
/// answered 404 (no row: never switched, so on).
pub fn enabled_from_response(response: Option<&str>) -> Result<bool, SwitchProblem> {
    let Some(body) = response else {
        return Ok(true);
    };
    let row: serde_json::Value = serde_json::from_str(body).map_err(|_| SwitchProblem)?;
    let stored = row.get("value").and_then(serde_json::Value::as_str).ok_or(SwitchProblem)?;
    Ok(hummingbird_domain::question_enabled_from_stored(stored))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(stored: &str) -> String {
        format!(r#"{{"key":"{SWITCH_KEY}","value":{},"updated_at":1,"version":2}}"#, serde_json::json!(stored))
    }

    #[test]
    fn the_key_is_the_cores_frozen_spelling() {
        assert_eq!(SWITCH_KEY, "question-enabled-fantasy");
    }

    #[test]
    fn no_row_means_the_question_is_on() {
        assert_eq!(enabled_from_response(None), Ok(true));
    }

    #[test]
    fn a_stored_false_switches_it_off_and_true_leaves_it_on() {
        assert_eq!(enabled_from_response(Some(&row("false"))), Ok(false));
        assert_eq!(enabled_from_response(Some(&row("true"))), Ok(true));
    }

    /// ADR-0034: a stored value this build cannot interpret must not
    /// silence a question.
    #[test]
    fn a_stored_value_this_build_cannot_read_leaves_it_on() {
        for stored in ["\"false\"", "0", "null", "{}", "not json"] {
            assert_eq!(enabled_from_response(Some(&row(stored))), Ok(true), "{stored}");
        }
    }

    #[test]
    fn an_unreadable_authority_answer_is_a_problem_not_an_answer() {
        for body in ["", "not json", "{}", r#"{"value":false}"#] {
            assert_eq!(enabled_from_response(Some(body)), Err(SwitchProblem), "{body:?}");
        }
    }
}
