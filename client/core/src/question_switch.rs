//! The **standing-question off switch** (#715, ADR-0034): one `settings`
//! row per question saying whether that question is asked at all.
//!
//! Off means *hidden, silent and unpolled* — the surface emits no pane, the
//! alert lane raises nothing, the poller makes no call. Those are three
//! layers reading one synced fact, which is why the fact is stored on the
//! authority rather than kept per device.
//!
//! # Why this is not a [`crate::bindings::BindingKey`]
//!
//! ADR-0034 decision 2, with two reasons this module has to keep honest.
//! [`crate::bindings::BindingValue`] is `Unset | Text | Other` and
//! [`crate::Core::set_binding`] writes only strings, so a boolean routed
//! through that path is the literal `"true"` in an editable free-text box —
//! a text field where a toggle belongs. And a binding is defined as *"the
//! small cross-device facts a pane needs before it can answer anything"*; a
//! switch is not a fact a pane reads, it decides whether the pane is
//! **asked**. So this is a second, typed vocabulary over the same table,
//! not a third meaning bolted onto the first — and
//! [`crate::Core::bindings`] filters these rows back out, or the editor
//! would list ten free-text `"false"`s beside the five real bindings.
//!
//! # Why one row per question, and why absence is enabled
//!
//! **Per question.** A `settings` row is written as an entity-level CAS set
//! on one version, so a phone switching fantasy off and a browser switching
//! race off would be two writes to the same version of one row — one loses,
//! and the loser is a toggle that visibly flips back. Per-question rows
//! make concurrent toggles of *different* questions structurally
//! non-conflicting (`Core::two_cores_toggling_two_questions_do_not_collide`
//! is that claim as a test, not an assertion). The cost is that `settings`
//! has no DELETE and so grows one permanent row per question ever switched
//! — bounded by the question vocabulary itself, which is the whole reason
//! [`question_switch_key`] is a closed match rather than a format string.
//!
//! **Absence is enabled** ([`question_enabled_from_stored`]). No migration,
//! no backfill, and a question a build has never heard of is on rather than
//! mysteriously off — the same reading [`crate::bindings::Binding::known`]
//! gives an unrecognised key. A row holding something that is not `false`
//! reads as enabled for the same reason: a value this build cannot
//! interpret must not silence a question.
//!
//! # The spelling is frozen
//!
//! `settings` has no DELETE, so respelling a key later orphans every row
//! written under the old one — permanently, and invisibly, since the
//! orphan reads as "absent" and the question comes back on. The ten
//! spellings in [`question_switch_key`] are therefore written out as
//! literals rather than derived with `format!` from
//! [`StandingQuestion::as_str`]: a rename of a *question's* wire word (the
//! kind #714 already made once, to a label) must not silently re-key rows
//! the operator has already written. `the_ten_keys_are_frozen` is the pin.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::decisions::panes::contract::{StandingQuestion, QUESTION_ORDER};

/// The `settings.key` one question's switch is stored under — the wire
/// spelling in both directions, and frozen (see the module header).
///
/// Wildcard-free by design. This is the sixth `match` an eleventh
/// [`StandingQuestion`] fails to compile against, and the cheapest kind of
/// exhaustiveness gate there is: a test can be forgotten, a `match` arm
/// cannot (`decisions::questions`'s header records what that family of
/// gates does and does not reach).
pub fn question_switch_key(question: StandingQuestion) -> &'static str {
    match question {
        StandingQuestion::Homework => "question-enabled-homework",
        StandingQuestion::Scps => "question-enabled-scps",
        StandingQuestion::Waste => "question-enabled-waste",
        StandingQuestion::Weekend => "question-enabled-weekend",
        StandingQuestion::Vacation => "question-enabled-vacation",
        StandingQuestion::Race => "question-enabled-race",
        StandingQuestion::Kimi => "question-enabled-kimi",
        StandingQuestion::Github => "question-enabled-github",
        StandingQuestion::Uptime => "question-enabled-uptime",
        StandingQuestion::Reachability => "question-enabled-reachability",
    }
}

/// Resolves a `settings` key back to the question it switches, by name —
/// the same closed-by-name discipline [`crate::bindings::BindingKey::parse`]
/// applies, and for the same reason: no caller may mint a key into a table
/// with no DELETE.
///
/// `None` is simply "not a switch row": an ordinary binding, or a row a
/// newer build wrote.
pub fn parse_question_switch_key(key: &str) -> Option<StandingQuestion> {
    QUESTION_ORDER
        .iter()
        .copied()
        .find(|question| question_switch_key(*question) == key)
}

/// How one stored `settings.value` reads as an on/off state.
///
/// Only the JSON literal `false` switches a question off. Absence is
/// handled by the caller (there is no row to pass here); anything else —
/// `true`, a string, an object, unparseable bytes — reads as **enabled**,
/// on the module header's "a value this build cannot interpret must not
/// silence a question".
pub fn question_enabled_from_stored(raw: &str) -> bool {
    !matches!(
        serde_json::from_str::<serde_json::Value>(raw),
        Ok(serde_json::Value::Bool(false))
    )
}

/// One question's switch as a settings surface reads it — the
/// [`crate::bindings::Binding`]-shaped twin for this second vocabulary,
/// down to `pending` being the read-time overlay fact rather than a stored
/// column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuestionSwitch {
    /// The question's own wire spelling (`StandingQuestion::as_str`), never
    /// the `settings` key — a surface names a question, and the key is this
    /// module's business.
    pub question: String,
    /// Whether this question is asked at all. `true` with no row behind it
    /// is the ordinary case.
    pub enabled: bool,
    /// Whether an unconfirmed local write is currently overlaid on this
    /// question's row — [`crate::bindings::Binding::pending`] verbatim.
    pub pending: bool,
}

/// Which questions are switched **off**, as [`crate::decisions::panes`]
/// reads them: an applied result, in [`QUESTION_ORDER`], carrying the wire
/// spellings a `PaneInputs` crossing understands.
///
/// The disabled half rather than the enabled half deliberately: it is
/// almost always empty, and an empty list means "every question is asked",
/// which is the same absence-means-enabled reading one layer up.
pub fn disabled_questions(switches: &[QuestionSwitch]) -> Vec<String> {
    switches
        .iter()
        .filter(|switch| !switch.enabled)
        .map(|switch| switch.question.clone())
        .collect()
}

/// Every switch key this build writes — the set [`crate::Core::bindings`]
/// subtracts so the bindings editor never lists a switch row as a binding.
pub fn all_switch_keys() -> BTreeSet<&'static str> {
    QUESTION_ORDER.iter().copied().map(question_switch_key).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ten_keys_are_frozen() {
        // Written out rather than derived, and pinned rather than trusted:
        // `settings` has no DELETE, so a respelling orphans every row an
        // operator has already written and silently turns the question back
        // on. Changing a line here is a data migration, not a rename.
        let keys: Vec<&str> = QUESTION_ORDER.iter().map(|q| question_switch_key(*q)).collect();
        assert_eq!(
            keys,
            vec![
                "question-enabled-homework",
                "question-enabled-scps",
                "question-enabled-waste",
                "question-enabled-weekend",
                "question-enabled-vacation",
                "question-enabled-race",
                "question-enabled-kimi",
                "question-enabled-github",
                "question-enabled-uptime",
                "question-enabled-reachability",
            ]
        );
    }

    #[test]
    fn a_switch_key_round_trips_by_name_and_nothing_else_resolves() {
        for question in QUESTION_ORDER {
            assert_eq!(
                parse_question_switch_key(question_switch_key(question)),
                Some(question)
            );
        }
        // Every binding key, and near-misses of the switch vocabulary.
        for key in crate::bindings::BindingKey::ALL {
            assert_eq!(parse_question_switch_key(key.as_str()), None);
        }
        for key in ["", "question-enabled-", "question-enabled-fantasy", "race", "questionEnabledRace"] {
            assert_eq!(parse_question_switch_key(key), None, "{key}");
        }
    }

    #[test]
    fn a_switch_key_can_never_collide_with_a_binding_key() {
        // The two vocabularies share one table. A collision would mean the
        // bindings editor and the toggle writing the same row with
        // incompatible value types.
        let switches = all_switch_keys();
        assert_eq!(switches.len(), QUESTION_ORDER.len());
        for key in crate::bindings::BindingKey::ALL {
            assert!(!switches.contains(key.as_str()), "{}", key.as_str());
        }
    }

    #[test]
    fn only_the_json_literal_false_switches_a_question_off() {
        assert!(!question_enabled_from_stored("false"));
        // Everything a build might not understand reads as ON.
        for raw in ["true", "\"false\"", "0", "null", "{}", "", "not json"] {
            assert!(question_enabled_from_stored(raw), "{raw}");
        }
    }

    #[test]
    fn the_disabled_list_is_empty_when_everything_is_on() {
        let all_on: Vec<QuestionSwitch> = QUESTION_ORDER
            .iter()
            .map(|question| QuestionSwitch {
                question: question.as_str().to_string(),
                enabled: true,
                pending: false,
            })
            .collect();
        assert!(disabled_questions(&all_on).is_empty());

        let mut mixed = all_on;
        mixed[3].enabled = false;
        mixed[8].enabled = false;
        assert_eq!(disabled_questions(&mixed), vec!["weekend", "uptime"]);
    }

    #[test]
    fn a_switch_crosses_as_the_three_contract_fields_and_nothing_else() {
        assert_eq!(
            serde_json::to_string(&QuestionSwitch {
                question: "race".to_string(),
                enabled: false,
                pending: true,
            })
            .unwrap(),
            r#"{"question":"race","enabled":false,"pending":true}"#
        );
    }
}
