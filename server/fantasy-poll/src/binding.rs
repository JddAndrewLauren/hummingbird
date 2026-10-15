//! Which leagues to poll, read out of `GET /api/settings/yahoo-leagues`,
//! and which of them belong to the current season.
//!
//! **The value is comma-separated league keys inside the JSON string**
//! (`"449.l.123456"`, later `"449.l.123456,449.l.789012"`), split by the
//! core's own `yahoo_leagues_from_binding` through `hummingbird_domain` —
//! one parse, no twin. A JSON array would land as the client's `Other`
//! value state, which the shipped binding editor cannot write
//! (`race-poll/src/binding.rs` says why at length).
//!
//! # Unset, unconfigured and broken are three things
//!
//! race-poll's [`BindingProblem`] distinction, verbatim, plus the 404 as its
//! own arm so the lib can name it: **no row** and **a row that names no
//! league** are "nobody has configured this" — the caller exits 0 with
//! nothing to do — while **an answer from the authority that is not JSON**
//! is a real failure and stays loud.
//!
//! # The season check
//!
//! Yahoo league keys are **season-scoped**: `449.l.123456` is league
//! `123456` of game `449`, and the game key changes every August. A bound
//! key whose game is not the current one is **skipped and logged, never an
//! error** — race-poll's treatment of a series with no adapter — so that
//! league's panes render as a gap rather than vanishing or silently
//! monitoring nothing. The discovered-leagues snapshot
//! ([`crate::body::DiscoveredBody`]) is what makes the annual re-type a copy
//! rather than a hunt.

use std::fmt;

/// The binding key — the core's `fantasy::BINDING_KEY`, unversioned so a
/// `/v1 → /v2` source bump cannot orphan it.
pub const BINDING_KEY: &str = "yahoo-leagues";

/// Why the binding gave no league list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingProblem {
    /// The GET answered 404: nobody has bound a league. Nothing to do.
    Unset,
    /// The response, or the `value` inside it, was not JSON — a malformed
    /// answer from the authority. **Not** a configuration state.
    NotJson,
    /// The row holds something that is not text (a JSON array among them).
    /// Bound-but-unusable: a configuration state, not a failure.
    NotText,
    /// Set, and set to nothing (or nothing but separators) — `settings` has
    /// no DELETE, so a blanked row is how a binding is un-set.
    Empty,
}

impl BindingProblem {
    /// Whether this is "nobody has configured this" rather than "the lane is
    /// broken". The two must not share an exit code.
    pub fn is_unconfigured(&self) -> bool {
        !matches!(self, BindingProblem::NotJson)
    }
}

impl fmt::Display for BindingProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BindingProblem::Unset => write!(f, "`{BINDING_KEY}` is not set"),
            BindingProblem::NotJson => write!(f, "`{BINDING_KEY}` is not readable JSON"),
            BindingProblem::NotText => write!(f, "`{BINDING_KEY}` does not hold text"),
            BindingProblem::Empty => write!(f, "`{BINDING_KEY}` names no league"),
        }
    }
}

/// The bound league keys, given the body of `GET /api/settings/yahoo-leagues`
/// — or `None` when that GET answered 404.
pub fn leagues_from_response(response: Option<&str>) -> Result<Vec<String>, BindingProblem> {
    let body = response.ok_or(BindingProblem::Unset)?;
    let row: serde_json::Value = serde_json::from_str(body).map_err(|_| BindingProblem::NotJson)?;
    let stored = row
        .get("value")
        .and_then(serde_json::Value::as_str)
        .ok_or(BindingProblem::NotJson)?;
    let text = match serde_json::from_str::<serde_json::Value>(stored) {
        Ok(serde_json::Value::String(text)) => text,
        Ok(_) => return Err(BindingProblem::NotText),
        Err(_) => return Err(BindingProblem::NotJson),
    };
    let leagues = hummingbird_domain::yahoo_leagues_from_binding(&text);
    if leagues.is_empty() {
        return Err(BindingProblem::Empty);
    }
    Ok(leagues)
}

/// The game a league key belongs to — the part before `.l.` — or `None`
/// for a string that is not a league key at all.
pub fn game_key_of(league_key: &str) -> Option<&str> {
    let (game, league) = league_key.split_once(".l.")?;
    (!game.is_empty() && !league.is_empty()).then_some(game)
}

/// A bound league this run will not poll, and why — for the log line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedLeague {
    pub league_key: String,
    pub reason: SkipReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// The key belongs to another season's game.
    StaleSeason { game_key: String, current_game_key: String },
    /// The string has no `<game>.l.<league>` shape at all.
    NotALeagueKey,
}

impl fmt::Display for SkippedLeague {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.reason {
            SkipReason::StaleSeason { game_key, current_game_key } => write!(
                f,
                "skipped `{}`: it belongs to game {game_key}, not this season's {current_game_key} \
                 — re-type the binding from the discovered leagues",
                self.league_key
            ),
            SkipReason::NotALeagueKey => {
                write!(f, "skipped `{}`: not a Yahoo league key (`<game>.l.<league>`)", self.league_key)
            }
        }
    }
}

/// The bound leagues, split by season.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SeasonSplit {
    /// The keys this run polls, in binding order.
    pub current: Vec<String>,
    /// The keys it does not, each with its reason — logged, never an error.
    pub skipped: Vec<SkippedLeague>,
}

/// Splits the bound leagues on the current season's game key (which the
/// live half reads from Yahoo's `game` resource).
pub fn split_by_season(leagues: &[String], current_game_key: &str) -> SeasonSplit {
    let mut split = SeasonSplit::default();
    for league_key in leagues {
        let reason = match game_key_of(league_key) {
            Some(game) if game == current_game_key => {
                split.current.push(league_key.clone());
                continue;
            }
            Some(game) => SkipReason::StaleSeason {
                game_key: game.to_string(),
                current_game_key: current_game_key.to_string(),
            },
            None => SkipReason::NotALeagueKey,
        };
        split.skipped.push(SkippedLeague { league_key: league_key.clone(), reason });
    }
    split
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(stored: &str) -> String {
        format!(r#"{{"key":"yahoo-leagues","value":{},"updated_at":1,"version":3}}"#, serde_json::json!(stored))
    }

    #[test]
    fn the_key_is_the_one_the_pane_reads() {
        assert_eq!(BINDING_KEY, "yahoo-leagues");
    }

    #[test]
    fn an_unset_row_is_nothing_to_do() {
        let problem = leagues_from_response(None).unwrap_err();
        assert_eq!(problem, BindingProblem::Unset);
        assert!(problem.is_unconfigured());
    }

    #[test]
    fn one_current_season_key_is_polled() {
        let leagues = leagues_from_response(Some(&row(r#""449.l.123456""#))).unwrap();
        assert_eq!(leagues, vec!["449.l.123456"]);
        let split = split_by_season(&leagues, "449");
        assert_eq!(split.current, vec!["449.l.123456"]);
        assert!(split.skipped.is_empty());
    }

    /// The August case: last season's key is still bound. Skipped with a
    /// reason the binary logs — never an error, and never polled.
    #[test]
    fn a_stale_season_key_is_skipped_and_logged_not_an_error() {
        let leagues =
            leagues_from_response(Some(&row(r#""423.l.98765, 449.l.123456""#))).unwrap();
        let split = split_by_season(&leagues, "449");
        assert_eq!(split.current, vec!["449.l.123456"]);
        assert_eq!(
            split.skipped,
            vec![SkippedLeague {
                league_key: "423.l.98765".to_string(),
                reason: SkipReason::StaleSeason {
                    game_key: "423".to_string(),
                    current_game_key: "449".to_string(),
                },
            }]
        );
        let logged = split.skipped[0].to_string();
        assert!(logged.contains("423.l.98765") && logged.contains("449"), "{logged}");
    }

    #[test]
    fn a_string_that_is_no_league_key_is_skipped_too() {
        let split = split_by_season(&["my league".to_string(), ".l.1".to_string()], "449");
        assert!(split.current.is_empty());
        assert!(split.skipped.iter().all(|s| s.reason == SkipReason::NotALeagueKey));
    }

    /// The one arm that is NOT a configuration state: nothing about a
    /// malformed authority answer says the operator has not bound a league.
    #[test]
    fn malformed_authority_json_is_a_problem_not_a_skip() {
        for body in ["", "not json", "{}", r#"{"value":"not-json-inside"}"#] {
            let problem = leagues_from_response(Some(body)).unwrap_err();
            assert_eq!(problem, BindingProblem::NotJson, "{body:?}");
            assert!(!problem.is_unconfigured(), "{body:?}");
        }
    }

    #[test]
    fn a_row_that_is_not_text_or_is_blank_is_unconfigured() {
        for (stored, want) in [
            (r#"["449.l.1"]"#, BindingProblem::NotText),
            ("42", BindingProblem::NotText),
            (r#""""#, BindingProblem::Empty),
            (r#"" , ""#, BindingProblem::Empty),
        ] {
            let problem = leagues_from_response(Some(&row(stored))).unwrap_err();
            assert_eq!(problem, want, "{stored}");
            assert!(problem.is_unconfigured(), "{stored}");
        }
    }
}
