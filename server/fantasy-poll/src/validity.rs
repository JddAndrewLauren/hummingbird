//! Whether a starting slot is **decidably invalid** — the lineup subject's
//! whole state (#713's "Lineup" row: rings only when the lineup is invalid
//! and a lock is near, resolves by itself when the next poll sees it valid).
//!
//! Three rules, and only three, because each is a fact Yahoo states rather
//! than a judgement: the slot is **empty**, its starter is **on bye**, or
//! its starter is designated **Out** or **IR**. A questionable or doubtful
//! starter may play and is valid. Nothing here asks whether a legal lineup
//! is a good one — that needs projections and an opinion about football,
//! and this lane is a deadline monitor, not an advice engine.

use serde::{Deserialize, Serialize};

use crate::roster::{Designation, Roster, StartingSlot};

/// Why a starting slot is invalid. Serialized into the lineup body
/// ([`crate::body::SlotBody`]) as the pane's reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Invalid {
    Empty,
    Bye,
    Out,
    InjuredReserve,
}

/// The slot's problem in `week`, or `None` for a valid slot. The rules are
/// checked in the order above, so a starter who is both on bye and Out
/// reads as on bye — either way the slot needs filling.
pub fn slot_problem(slot: &StartingSlot, week: u8) -> Option<Invalid> {
    let Some(player) = &slot.occupant else {
        return Some(Invalid::Empty);
    };
    if player.bye_week == Some(week) {
        return Some(Invalid::Bye);
    }
    match player.designation {
        Some(Designation::Out) => Some(Invalid::Out),
        Some(Designation::InjuredReserve) => Some(Invalid::InjuredReserve),
        Some(Designation::Questionable | Designation::Doubtful) | None => None,
    }
}

/// Whether every starting slot is valid.
pub fn lineup_is_valid(roster: &Roster) -> bool {
    roster.starters.iter().all(|slot| slot_problem(slot, roster.week).is_none())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roster::{Player, Team};

    const WEEK: u8 = 4;

    fn player(designation: Option<Designation>, bye_week: u8) -> Player {
        Player {
            player_key: "449.p.1".to_string(),
            name: "A Starter".to_string(),
            eligible_positions: vec!["WR".to_string()],
            bye_week: Some(bye_week),
            designation,
            kickoff_ms: Some(1_791_133_200_000),
        }
    }

    fn slot(occupant: Option<Player>) -> StartingSlot {
        StartingSlot { position: "WR".to_string(), occupant }
    }

    fn roster(starters: Vec<StartingSlot>) -> Roster {
        Roster {
            team: Team {
                team_key: "449.l.123456.t.7".to_string(),
                league_key: "449.l.123456".to_string(),
                team_name: "Team".to_string(),
                league_name: "League".to_string(),
            },
            week: WEEK,
            starters,
            bench: vec![],
        }
    }

    #[test]
    fn an_empty_slot_is_invalid() {
        assert_eq!(slot_problem(&slot(None), WEEK), Some(Invalid::Empty));
    }

    #[test]
    fn a_starter_on_bye_is_invalid() {
        assert_eq!(slot_problem(&slot(Some(player(None, WEEK))), WEEK), Some(Invalid::Bye));
    }

    #[test]
    fn a_starter_designated_out_is_invalid() {
        let out = slot(Some(player(Some(Designation::Out), 9)));
        assert_eq!(slot_problem(&out, WEEK), Some(Invalid::Out));
    }

    #[test]
    fn a_starter_on_ir_is_invalid() {
        let ir = slot(Some(player(Some(Designation::InjuredReserve), 9)));
        assert_eq!(slot_problem(&ir, WEEK), Some(Invalid::InjuredReserve));
    }

    /// Questionable means he may play. Starting him is a judgement call,
    /// and judgement calls are out of scope.
    #[test]
    fn a_questionable_starter_is_not_invalid() {
        let questionable = slot(Some(player(Some(Designation::Questionable), 9)));
        assert_eq!(slot_problem(&questionable, WEEK), None);
        let doubtful = slot(Some(player(Some(Designation::Doubtful), 9)));
        assert_eq!(slot_problem(&doubtful, WEEK), None);
    }

    #[test]
    fn a_fully_valid_roster_is_valid() {
        let valid = roster(vec![
            slot(Some(player(None, 9))),
            slot(Some(player(Some(Designation::Questionable), 11))),
        ]);
        assert!(lineup_is_valid(&valid));
        let invalid = roster(vec![slot(Some(player(None, 9))), slot(None)]);
        assert!(!lineup_is_valid(&invalid));
    }
}
