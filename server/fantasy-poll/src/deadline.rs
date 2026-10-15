//! The last moment an invalid lineup can still be fixed.
//!
//! Yahoo lets a slot be filled until the game of the player being
//! **inserted** kicks off. So an invalid slot's deadline is the earliest
//! kickoff among that slot's **eligible replacements**, and the lineup's is
//! the minimum across its invalid slots (#713's "Rings" section). It is
//! conservative by construction: if the operator fixes the slot with a
//! later-kicking replacement they had more time than this says — early,
//! never late.
//!
//! An **eligible replacement** is a bench player who may fill the slot's
//! position and whose game has not kicked off at the fetch instant
//! (`as_of_ms`): a player whose game has started is locked where he is. A
//! replacement who is himself Out stays eligible — counting him can only
//! make the deadline earlier. Reserve players never appear on the bench
//! ([`crate::roster::Roster`]), and a player on bye has no kickoff, so
//! neither can set a deadline.
//!
//! An invalid slot with no eligible replacement on the roster has **no
//! deadline** (`None`): nothing on the roster can fix it, and inventing an
//! instant would be a guess.

use crate::roster::{Player, Roster, StartingSlot};
use crate::validity::slot_problem;

/// The earliest kickoff, after `as_of_ms`, among the bench players who may
/// fill `slot`.
pub fn earliest_replacement_kickoff_ms(
    slot: &StartingSlot,
    bench: &[Player],
    as_of_ms: i64,
) -> Option<i64> {
    bench
        .iter()
        .filter(|player| player.fits(&slot.position))
        .filter_map(|player| player.kickoff_ms)
        .filter(|kickoff| *kickoff > as_of_ms)
        .min()
}

/// One invalid slot's deadline: the earliest eligible-replacement kickoff.
/// The one place a slot's deadline is decided — the lineup minimum and the
/// body both read it from here.
pub fn slot_deadline_ms(slot: &StartingSlot, bench: &[Player], as_of_ms: i64) -> Option<i64> {
    earliest_replacement_kickoff_ms(slot, bench, as_of_ms)
}

/// The lineup's deadline: the minimum of its invalid slots' deadlines, or
/// `None` when it is valid or no invalid slot can be fixed from the roster.
pub fn lineup_deadline_ms(roster: &Roster, as_of_ms: i64) -> Option<i64> {
    roster
        .starters
        .iter()
        .filter(|slot| slot_problem(slot, roster.week).is_some())
        .filter_map(|slot| slot_deadline_ms(slot, &roster.bench, as_of_ms))
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed fixture roster (week 4, fetched Tue 2026-09-29 15:00Z):
    /// an empty RB slot, a WR starter on bye and a W/R/T starter who is Out.
    /// The bench holds an RB in a Sunday 1pm game, a WR in Thursday night's
    /// game, a TE in a Sunday 9:30am London game and a WR on Monday night.
    const ROSTER: &str = include_str!("../tests/fixtures/roster.json");
    const AS_OF_MS: i64 = 1_790_694_000_000;
    const THURSDAY_NIGHT_MS: i64 = 1_790_900_100_000;
    const SUNDAY_LONDON_MS: i64 = 1_791_120_600_000;
    const SUNDAY_1PM_MS: i64 = 1_791_133_200_000;

    fn roster() -> Roster {
        serde_json::from_str(ROSTER).expect("the fixture is a roster")
    }

    fn slot(roster: &Roster, position: &str) -> StartingSlot {
        roster
            .starters
            .iter()
            .find(|slot| slot.position == position)
            .unwrap_or_else(|| panic!("the fixture starts a {position}"))
            .clone()
    }

    /// The acceptance fixture: three invalid slots, and the earliest
    /// deadline among them belongs to a **bench** WR in Thursday night's
    /// early game — not to any starter, and not to the first invalid slot.
    #[test]
    fn the_lineup_deadline_is_the_earliest_replacement_kickoff_across_invalid_slots() {
        let roster = roster();
        assert_eq!(lineup_deadline_ms(&roster, AS_OF_MS), Some(THURSDAY_NIGHT_MS));
    }

    #[test]
    fn each_invalid_slot_takes_the_earliest_of_its_own_replacements() {
        let roster = roster();
        // The empty RB slot: only the bench RB fits.
        assert_eq!(slot_deadline_ms(&slot(&roster, "RB"), &roster.bench, AS_OF_MS), Some(SUNDAY_1PM_MS));
        // The WR on bye: Thursday's WR beats Monday's.
        assert_eq!(slot_deadline_ms(&slot(&roster, "WR"), &roster.bench, AS_OF_MS), Some(THURSDAY_NIGHT_MS));
        // The flex: RB, WR and TE all fit; the WR's Thursday game is first,
        // ahead of the TE's London kickoff.
        assert_eq!(
            slot_deadline_ms(&slot(&roster, "W/R/T"), &roster.bench, AS_OF_MS),
            Some(THURSDAY_NIGHT_MS)
        );
    }

    /// Once Thursday's game has kicked off that WR is locked, so the next
    /// fetch's deadline moves to the next eligible kickoff — never one in
    /// the past.
    #[test]
    fn a_replacement_whose_game_has_started_is_no_longer_eligible() {
        let roster = roster();
        assert_eq!(lineup_deadline_ms(&roster, THURSDAY_NIGHT_MS), Some(SUNDAY_LONDON_MS));
    }

    #[test]
    fn a_slot_nothing_on_the_bench_can_fill_has_no_deadline() {
        let mut roster = roster();
        roster.bench.clear();
        assert_eq!(lineup_deadline_ms(&roster, AS_OF_MS), None);
    }

    #[test]
    fn a_valid_lineup_has_no_deadline() {
        let mut roster = roster();
        roster.starters.retain(|slot| slot_problem(slot, roster.week).is_none());
        assert_eq!(lineup_deadline_ms(&roster, AS_OF_MS), None);
    }
}
