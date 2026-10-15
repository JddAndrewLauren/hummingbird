//! The two `context_snapshots` payloads this poller writes under
//! `yahoo-lineup/v1`, each ADR-0015's envelope around a source-shaped body:
//!
//! | `key` | `schema` | body |
//! | --- | --- | --- |
//! | the league key (`449.l.123456`) | `yahoo-lineup/v1` | [`LineupBody`] — one team's lineup this week |
//! | [`DISCOVERED_KEY`] | [`DISCOVERED_SCHEMA`] | [`DiscoveredBody`] — every league the operator has a team in this season |
//!
//! One source, two keys — `google-calendar/v1`'s cursor-beside-gauge shape
//! (`calendar-poll/src/cursor.rs`). The lineup row is keyed by the
//! **league**, because that is what the pane looks up
//! (`client/core/src/decisions/panes/fantasy.rs`'s `fantasy_facts`); a
//! league key always carries `.l.`, so [`DISCOVERED_KEY`] cannot collide
//! with one. The alert's `source_key` is a different thing — the team,
//! `team:<team_key>` (`hummingbird_domain::yahoo_lineup_v1_key`), #720's.
//!
//! **`tests/fixtures/golden-lineup.json` and `golden-discovered.json` pin
//! both bodies byte for byte.** They are the contract #855's pane parser and
//! Settings' discovered-leagues line are written against; a diff there is a
//! decision about that contract, not a chore.
//!
//! # What the lineup body carries
//!
//! Per **starting** slot: its position, its occupant (`null` when the slot
//! is empty), the occupant's bye week, injury designation and kickoff, and
//! — when the slot is invalid — why, and its deadline
//! ([`crate::deadline::slot_deadline_ms`]). Plus the team key, league key,
//! both names and the week: enough for a headline like "Lineup set — locks
//! Sun 1:00pm" or "WR on bye — fix by Thu 8:15pm" without a second read.
//! The bench is not carried; its only use here is the deadlines, which are
//! decided at fetch time against the fetch instant.
//!
//! **The lineup's own deadline is not stored** — it is the minimum over the
//! slots' and [`LineupBody::deadline_ms`] derives it on read, so the stored
//! row cannot disagree with itself.
//!
//! **No `zone`.** Every instant here is an instant (epoch ms); turning one
//! into "Sun 1:00pm" is a read-time, device-local act (ADR-0015's Time).

use serde::{Deserialize, Serialize};

use hummingbird_domain::{SnapshotEnvelope, YAHOO_LINEUP_V1};

use crate::deadline::slot_deadline_ms;
use crate::roster::{Designation, Player, Roster, Team};
use crate::validity::{slot_problem, Invalid};

/// The cadence this poller declares, for `Freshness`.
///
/// **A floor, not the nominal interval** (ADR-0015, amended inline at
/// #853): the binary runs every 15 minutes and fetches only when
/// [`crate::gate`] says so, but it always fetches once the last fetch is
/// over this old. So no row is ever older than ~6h plus a run's slack, and
/// ADR-0015's `2 ×` rule reads 12h — a margin Actions' cron jitter never
/// trips. Declaring 15 minutes instead would make every quiet hour read
/// stale.
pub const POLLED_EVERY_MS: i64 = 6 * 60 * 60 * 1000;

/// The discovered-leagues row's `context_snapshots.key`.
pub const DISCOVERED_KEY: &str = "discovered-leagues";

/// The discovered-leagues payload's own versioned name (ADR-0015's envelope
/// `schema`) — not the source's, because it is not a lineup; the pane reads
/// a non-lineup schema under a league key as a gap, and never reads this key.
pub const DISCOVERED_SCHEMA: &str = "yahoo-discovered-leagues/v1";

/// A starting slot's occupant, as the pane shows him.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OccupantBody {
    pub player_key: String,
    pub name: String,
    pub bye_week: Option<u8>,
    pub designation: Option<Designation>,
    /// His game's kickoff this week; `null` on a bye.
    pub kickoff_ms: Option<i64>,
}

/// Why a slot is invalid, and until when it can be fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvalidSlot {
    pub reason: Invalid,
    /// The earliest kickoff among the slot's eligible replacements, as of
    /// the fetch; `null` when nothing on the roster can fill it.
    pub deadline_ms: Option<i64>,
}

/// One starting slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotBody {
    pub position: String,
    /// `null` is an empty slot.
    pub occupant: Option<OccupantBody>,
    /// `null` is a valid slot.
    pub invalid: Option<InvalidSlot>,
}

/// The `yahoo-lineup/v1` body: one team's lineup for one week.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineupBody {
    pub team_key: String,
    pub team_name: String,
    pub league_key: String,
    pub league_name: String,
    pub week: u8,
    pub slots: Vec<SlotBody>,
}

/// Why a stored payload could not be read back as a lineup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyProblem {
    /// The ADR-0015 envelope itself did not parse, in its own words.
    Envelope(String),
    /// The envelope's `schema` is not `yahoo-lineup/v1`.
    WrongSchema(String),
    /// The envelope's `body` is not a lineup body.
    Body(String),
}

impl std::fmt::Display for BodyProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BodyProblem::Envelope(reason) => write!(f, "snapshot envelope: {reason}"),
            BodyProblem::WrongSchema(schema) => {
                write!(f, "snapshot carries `{schema}`, not `{YAHOO_LINEUP_V1}`")
            }
            BodyProblem::Body(reason) => write!(f, "snapshot body: {reason}"),
        }
    }
}

fn occupant(player: &Player) -> OccupantBody {
    OccupantBody {
        player_key: player.player_key.clone(),
        name: player.name.clone(),
        bye_week: player.bye_week,
        designation: player.designation,
        kickoff_ms: player.kickoff_ms,
    }
}

impl LineupBody {
    /// The body for one roster, its deadlines decided as of the fetch
    /// instant.
    pub fn from_roster(roster: &Roster, as_of_ms: i64) -> LineupBody {
        let Team { team_key, team_name, league_key, league_name } = roster.team.clone();
        let slots = roster
            .starters
            .iter()
            .map(|slot| SlotBody {
                position: slot.position.clone(),
                occupant: slot.occupant.as_ref().map(occupant),
                invalid: slot_problem(slot, roster.week).map(|reason| InvalidSlot {
                    reason,
                    deadline_ms: slot_deadline_ms(slot, &roster.bench, as_of_ms),
                }),
            })
            .collect();
        LineupBody { team_key, team_name, league_key, league_name, week: roster.week, slots }
    }

    /// The row's `context_snapshots.key`: the league, which is what the
    /// pane looks the row up by.
    pub fn snapshot_key(&self) -> &str {
        &self.league_key
    }

    /// Whether every starting slot is valid.
    pub fn is_valid(&self) -> bool {
        self.slots.iter().all(|slot| slot.invalid.is_none())
    }

    /// The lineup's deadline: the minimum over its invalid slots', derived on
    /// read rather than stored.
    pub fn deadline_ms(&self) -> Option<i64> {
        self.slots.iter().filter_map(|slot| slot.invalid?.deadline_ms).min()
    }

    /// ADR-0015's envelope — the `payload` of a `POST /api/snapshots`.
    pub fn envelope(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": YAHOO_LINEUP_V1,
            "polled_every_ms": POLLED_EVERY_MS,
            "body": self,
        })
    }

    /// Reads a lineup back out of a stored row's `payload` text — the
    /// fetch gate's input, exactly as [`LineupBody::envelope`] wrote it.
    pub fn from_payload(payload: &str) -> Result<LineupBody, BodyProblem> {
        let envelope =
            SnapshotEnvelope::parse(payload).map_err(|p| BodyProblem::Envelope(p.to_string()))?;
        if envelope.schema != YAHOO_LINEUP_V1 {
            return Err(BodyProblem::WrongSchema(envelope.schema));
        }
        serde_json::from_str(&envelope.body).map_err(|e| BodyProblem::Body(e.to_string()))
    }
}

/// One league the operator has a team in this season.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredLeague {
    /// The key to type into the `yahoo-leagues` binding.
    pub league_key: String,
    /// What to recognise it by.
    pub league_name: String,
}

/// The discovered-leagues body: the current season's game key and every
/// league the operator has a team in, sorted by league key so the row is
/// stable across runs whatever order Yahoo lists teams in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredBody {
    pub game_key: String,
    pub leagues: Vec<DiscoveredLeague>,
}

impl DiscoveredBody {
    /// The body for the operator's current-season teams. Two teams in one
    /// league are one discovered league.
    pub fn from_teams(game_key: &str, teams: &[Team]) -> DiscoveredBody {
        let mut leagues: Vec<DiscoveredLeague> = Vec::new();
        for team in teams {
            if !leagues.iter().any(|l| l.league_key == team.league_key) {
                leagues.push(DiscoveredLeague {
                    league_key: team.league_key.clone(),
                    league_name: team.league_name.clone(),
                });
            }
        }
        leagues.sort_by(|a, b| a.league_key.cmp(&b.league_key));
        DiscoveredBody { game_key: game_key.to_string(), leagues }
    }

    /// ADR-0015's envelope, under [`DISCOVERED_SCHEMA`].
    pub fn envelope(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": DISCOVERED_SCHEMA,
            "polled_every_ms": POLLED_EVERY_MS,
            "body": self,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROSTER: &str = include_str!("../tests/fixtures/roster.json");
    const AS_OF_MS: i64 = 1_790_694_000_000;

    fn lineup() -> LineupBody {
        LineupBody::from_roster(&serde_json::from_str(ROSTER).unwrap(), AS_OF_MS)
    }

    /// `SnapshotEnvelope::parse` is the exact check `POST /api/snapshots`
    /// runs, so both payloads must survive it — and both must declare the
    /// six-hour floor.
    #[test]
    fn both_envelopes_pass_the_authoritys_own_parse_and_declare_six_hours() {
        assert_eq!(POLLED_EVERY_MS, 6 * 60 * 60 * 1000);
        let lineup = SnapshotEnvelope::parse(&lineup().envelope().to_string()).unwrap();
        assert_eq!(lineup.schema, "yahoo-lineup/v1");
        assert_eq!(lineup.polled_every_ms, Some(POLLED_EVERY_MS));
        let discovered = DiscoveredBody::from_teams("449", &[]).envelope().to_string();
        let discovered = SnapshotEnvelope::parse(&discovered).unwrap();
        assert_eq!(discovered.schema, DISCOVERED_SCHEMA);
        assert_eq!(discovered.polled_every_ms, Some(POLLED_EVERY_MS));
    }

    #[test]
    fn a_written_lineup_reads_back_unchanged() {
        let payload = lineup().envelope().to_string();
        assert_eq!(LineupBody::from_payload(&payload), Ok(lineup()));
    }

    #[test]
    fn a_payload_under_another_schema_is_refused_by_name() {
        let payload = DiscoveredBody::from_teams("449", &[]).envelope().to_string();
        assert_eq!(
            LineupBody::from_payload(&payload),
            Err(BodyProblem::WrongSchema(DISCOVERED_SCHEMA.to_string()))
        );
        assert!(matches!(LineupBody::from_payload("not json"), Err(BodyProblem::Envelope(_))));
    }

    #[test]
    fn the_lineup_row_is_keyed_by_league_and_cannot_meet_the_discovered_key() {
        assert_eq!(lineup().snapshot_key(), "449.l.123456");
        assert!(!DISCOVERED_KEY.contains(".l."));
    }

    #[test]
    fn the_stored_deadline_is_the_minimum_over_invalid_slots() {
        let lineup = lineup();
        assert!(!lineup.is_valid());
        assert_eq!(lineup.deadline_ms(), Some(1_790_900_100_000));
    }

    #[test]
    fn two_teams_in_one_league_are_one_discovered_league_sorted_by_key() {
        let team = |team_key: &str, league_key: &str| Team {
            team_key: team_key.to_string(),
            league_key: league_key.to_string(),
            team_name: "T".to_string(),
            league_name: format!("L {league_key}"),
        };
        let body = DiscoveredBody::from_teams(
            "449",
            &[team("449.l.9.t.1", "449.l.9"), team("449.l.2.t.1", "449.l.2"), team("449.l.9.t.4", "449.l.9")],
        );
        let keys: Vec<&str> = body.leagues.iter().map(|l| l.league_key.as_str()).collect();
        assert_eq!(keys, vec!["449.l.2", "449.l.9"]);
    }
}
