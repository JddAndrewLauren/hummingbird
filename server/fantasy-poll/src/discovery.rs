//! Given the operator's teams in the current season, which bound leagues
//! get a lineup snapshot — and which leagues were discovered at all.
//!
//! **One team per bound league, or no snapshot.** The lineup subject is a
//! team's lineup, and a league holding two of the operator's teams (a
//! co-managed team, a second account's team surfacing through the same
//! login) leaves "which lineup?" with no decidable answer. That is a
//! **loud failure**: no snapshot for that league and a logged reason naming
//! both teams — never a guess at the first one. A bound league the operator
//! has no team in gets no snapshot either, logged the same way.
//!
//! **The discovered leagues are written whatever the binding says** — they
//! are the August ritual's answer ("which keys should I bind this season?")
//! and must exist precisely when the binding is stale.

use std::fmt;

use crate::roster::Team;

/// What one bound, current-season league resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeagueOutcome {
    /// Exactly one of the operator's teams plays in it: snapshot its lineup.
    Snapshot(Team),
    /// None of the operator's teams plays in it. No snapshot, logged.
    NoTeam,
    /// More than one does. No snapshot, logged loudly — never a guess.
    SeveralTeams(Vec<String>),
}

/// One bound league and what it resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub league_key: String,
    pub outcome: LeagueOutcome,
}

impl Resolved {
    /// The team to snapshot, when there is exactly one.
    pub fn team(&self) -> Option<&Team> {
        match &self.outcome {
            LeagueOutcome::Snapshot(team) => Some(team),
            LeagueOutcome::NoTeam | LeagueOutcome::SeveralTeams(_) => None,
        }
    }
}

impl fmt::Display for Resolved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let league = &self.league_key;
        match &self.outcome {
            LeagueOutcome::Snapshot(team) => write!(f, "`{league}`: snapshotting team `{}`", team.team_key),
            LeagueOutcome::NoTeam => {
                write!(f, "`{league}`: no snapshot — none of your teams plays in this league")
            }
            LeagueOutcome::SeveralTeams(teams) => write!(
                f,
                "`{league}`: no snapshot — {} of your teams play in this league ({}), and which \
                 lineup to watch is not decidable",
                teams.len(),
                teams.join(", ")
            ),
        }
    }
}

/// Resolves each bound current-season league against the operator's teams,
/// in binding order.
pub fn resolve(current_leagues: &[String], teams: &[Team]) -> Vec<Resolved> {
    current_leagues
        .iter()
        .map(|league_key| {
            let mine: Vec<&Team> = teams.iter().filter(|t| &t.league_key == league_key).collect();
            let outcome = match mine.as_slice() {
                [] => LeagueOutcome::NoTeam,
                [team] => LeagueOutcome::Snapshot((*team).clone()),
                several => LeagueOutcome::SeveralTeams(several.iter().map(|t| t.team_key.clone()).collect()),
            };
            Resolved { league_key: league_key.clone(), outcome }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team(team_key: &str, league_key: &str) -> Team {
        Team {
            team_key: team_key.to_string(),
            league_key: league_key.to_string(),
            team_name: format!("Team {team_key}"),
            league_name: format!("League {league_key}"),
        }
    }

    #[test]
    fn one_team_in_a_bound_league_gets_a_snapshot() {
        let teams = [team("449.l.1.t.3", "449.l.1"), team("449.l.2.t.5", "449.l.2")];
        let resolved = resolve(&["449.l.1".to_string()], &teams);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].team(), Some(&teams[0]));
    }

    /// The acceptance case: two of the operator's teams in one bound league
    /// is no snapshot for that league and a logged reason naming both —
    /// while the other bound league is unaffected.
    #[test]
    fn two_teams_in_one_bound_league_yield_no_snapshot_and_a_logged_reason() {
        let teams = [
            team("449.l.1.t.3", "449.l.1"),
            team("449.l.1.t.9", "449.l.1"),
            team("449.l.2.t.5", "449.l.2"),
        ];
        let resolved = resolve(&["449.l.1".to_string(), "449.l.2".to_string()], &teams);
        assert_eq!(
            resolved[0].outcome,
            LeagueOutcome::SeveralTeams(vec!["449.l.1.t.3".to_string(), "449.l.1.t.9".to_string()])
        );
        assert_eq!(resolved[0].team(), None);
        let logged = resolved[0].to_string();
        assert!(
            logged.contains("449.l.1.t.3") && logged.contains("449.l.1.t.9") && logged.contains("no snapshot"),
            "{logged}"
        );
        assert_eq!(resolved[1].team(), Some(&teams[2]), "the other league is unaffected");
    }

    #[test]
    fn a_bound_league_with_none_of_my_teams_yields_no_snapshot() {
        let resolved = resolve(&["449.l.404".to_string()], &[team("449.l.1.t.3", "449.l.1")]);
        assert_eq!(resolved[0].outcome, LeagueOutcome::NoTeam);
        assert!(resolved[0].to_string().contains("no snapshot"));
    }
}
