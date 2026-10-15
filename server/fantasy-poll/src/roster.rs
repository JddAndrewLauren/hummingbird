//! The typed inputs every decision here reads — what the live half (#854)
//! parses Yahoo's responses **into**. Nothing in this crate reads Yahoo's
//! own wire shape; that seam is #854's to pin against a saved sample.
//!
//! The types say only what the decidable rules need: which slots start,
//! who fills them, who is on the bench, each player's bye week, injury
//! designation and kickoff this week. Projections, matchups and points are
//! absent on purpose — #713's "bad but legal" is out of scope, and a field
//! nothing reads is a field that drifts.
//!
//! `Deserialize` is for the committed fixtures (`tests/fixtures/roster.json`):
//! a test reads a roster in this crate's own shape, not Yahoo's.

use serde::{Deserialize, Serialize};

/// One of the operator's teams in the current season — what discovery
/// (`users;use_login=1/games/teams`, #854) yields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Team {
    /// `449.l.123456.t.7` — the lineup alert's `team:<team_key>` subject.
    pub team_key: String,
    /// `449.l.123456` — the league this team plays in.
    pub league_key: String,
    pub team_name: String,
    pub league_name: String,
}

/// An injury designation this lane distinguishes. Only [`Designation::Out`]
/// and [`Designation::InjuredReserve`] make a starter invalid;
/// questionable and doubtful players may play, and whether to start them is
/// an opinion about football. A designation outside this list is the live
/// half's to map (#854).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Designation {
    Questionable,
    Doubtful,
    Out,
    InjuredReserve,
}

/// One rostered player, as this week sees him.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub player_key: String,
    pub name: String,
    /// Every slot position he may fill — Yahoo lists the flex positions
    /// (`W/R/T`) here beside the base one, so eligibility is a lookup.
    pub eligible_positions: Vec<String>,
    /// His team's bye week this season, when known.
    pub bye_week: Option<u8>,
    pub designation: Option<Designation>,
    /// Kickoff of his team's game this week, epoch ms. `None` when his team
    /// does not play this week (a bye).
    pub kickoff_ms: Option<i64>,
}

/// One starting slot: its position and whoever fills it. `occupant: None`
/// is an empty slot — Yahoo lists players, not slots, so the live half
/// derives empty slots from the league's roster positions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartingSlot {
    /// The slot's position (`QB`, `WR`, `W/R/T`, `DEF`, …).
    pub position: String,
    pub occupant: Option<Player>,
}

/// One team's roster for one week. Reserve slots (`IR`, `NA`) are left out
/// by the live half: a reserve player neither starts nor can be inserted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Roster {
    pub team: Team,
    /// The league's current week.
    pub week: u8,
    pub starters: Vec<StartingSlot>,
    pub bench: Vec<Player>,
}

impl Player {
    /// Whether he may fill a slot of this position.
    pub fn fits(&self, position: &str) -> bool {
        self.eligible_positions.iter().any(|p| p == position)
    }
}
