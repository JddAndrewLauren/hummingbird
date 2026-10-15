//! The Fantasy question's facts that **both sides read** — the client's
//! panes (`client/core/src/decisions/panes/fantasy.rs`, #717) and the
//! Yahoo poller (`server/fantasy-poll`, #853) — moved down here at #853 so
//! there is one parse and no twin. `client/core` keeps re-exporting both
//! under their old paths; `StandingQuestion` and its exhaustive switch-key
//! `match` stay in the core, which delegates to [`FANTASY_SWITCH_KEY`].
//!
//! Neither is an answer: the league list is what the operator bound, and
//! the switch is whether the question is asked at all (ADR-0034).

/// The `settings.key` the Fantasy question's off switch is stored under
/// (#715, ADR-0034 decision 2). **Frozen**: `settings` has no DELETE, so a
/// respelling orphans every row already written and silently turns the
/// question back on — `client/core/src/question_switch.rs`'s header says
/// so at length, and its `the_twelve_keys_are_frozen` pins this literal.
pub const FANTASY_SWITCH_KEY: &str = "question-enabled-fantasy";

/// How one stored `settings.value` reads as an on/off state.
///
/// Only the JSON literal `false` switches a question off. Absence is the
/// caller's (there is no row to pass here) and reads as enabled; anything
/// else — `true`, a string, an object, unparseable bytes — reads as
/// **enabled** too: a value this build cannot interpret must not silence a
/// question (ADR-0034).
pub fn question_enabled_from_stored(raw: &str) -> bool {
    !matches!(
        serde_json::from_str::<serde_json::Value>(raw),
        Ok(serde_json::Value::Bool(false))
    )
}

/// The followed leagues, read out of the `yahoo-leagues` binding's text:
/// trimmed, blanks dropped, repeats dropped, order kept.
///
/// The binding holds **comma-separated league keys inside the JSON
/// string** — a JSON array would land as the client's `BindingValue::Other`,
/// which the binding editor cannot write (`server/race-poll/src/binding.rs`
/// says why at length). Unlike race's series list it is **not lowercased**:
/// a race series is a name a human types in any case, a league key is an
/// identifier copied from Yahoo and is taken exactly as written.
pub fn yahoo_leagues_from_binding(text: &str) -> Vec<String> {
    let mut leagues: Vec<String> = Vec::new();
    for entry in text.split(',') {
        let key = entry.trim();
        if key.is_empty() || leagues.iter().any(|league| league == key) {
            continue;
        }
        leagues.push(key.to_string());
    }
    leagues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_json_literal_false_switches_the_question_off() {
        assert!(!question_enabled_from_stored("false"));
        for raw in ["true", "\"false\"", "0", "null", "{}", "", "not json"] {
            assert!(question_enabled_from_stored(raw), "{raw}");
        }
    }

    #[test]
    fn the_league_list_is_trimmed_deduplicated_and_kept_in_order() {
        assert_eq!(
            yahoo_leagues_from_binding(" 449.l.2 ,,449.l.1, 449.l.2,"),
            vec!["449.l.2", "449.l.1"],
        );
        assert!(yahoo_leagues_from_binding(" , ").is_empty());
    }
}
