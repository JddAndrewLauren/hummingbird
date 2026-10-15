//! The last moment an invalid lineup can still be fixed.
//!
//! Yahoo lets a slot be filled until the game of the player being
//! **inserted** kicks off. So an invalid slot's deadline is the earliest
//! kickoff among that slot's **eligible replacements**, and the lineup's is
//! the minimum across its invalid slots (#713's "Rings" section).
//!
//! **Capped at the slot's own occupant's kickoff** (operator ruling, #853).
//! Yahoo also locks the player being *removed* once his game starts, so an
//! Out or IR starter whose game kicks off before every replacement's pins
//! the slot shut at his own kickoff. Without the cap the deadline would name
//! a later instant, after the slot can no longer be fixed. An empty slot or
//! a starter on bye has no kickoff and no cap. A starter whose game had
//! already kicked off at the fetch is locked in: that slot has **no
//! deadline**, because nothing can fix it any more.
//!
//! With the cap it is conservative by construction: if the operator fixes
//! the slot with a later-kicking replacement they had more time than this
//! says — early, never late.
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

/// One invalid slot's deadline: the earliest eligible-replacement kickoff,
/// capped at the occupant's own kickoff — or `None` when the occupant is
/// already locked in, or nothing on the bench can fill the slot. The one
/// place a slot's deadline is decided — the lineup minimum and the body
/// both read it from here.
pub fn slot_deadline_ms(slot: &StartingSlot, bench: &[Player], as_of_ms: i64) -> Option<i64> {
    let occupant_kickoff_ms = slot.occupant.as_ref().and_then(|player| player.kickoff_ms);
    if occupant_kickoff_ms.is_some_and(|kickoff| kickoff <= as_of_ms) {
        return None;
    }
    let replacement_ms = earliest_replacement_kickoff_ms(slot, bench, as_of_ms)?;
    Some(occupant_kickoff_ms.map_or(replacement_ms, |kickoff| kickoff.min(replacement_ms)))
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

    /// The operator's #853 ruling, on its own fixture: a WR starter who is
    /// Out plays Thursday night, and every bench WR plays on Sunday. Yahoo
    /// locks him into the slot at Thursday's kickoff, so that is the
    /// deadline — not the London game, which would be three days late.
    #[test]
    fn an_out_or_ir_starter_who_kicks_off_first_caps_his_slots_deadline() {
        let mut roster: Roster =
            serde_json::from_str(include_str!("../tests/fixtures/roster-starter-locks-first.json"))
                .expect("the fixture is a roster");
        assert_eq!(lineup_deadline_ms(&roster, AS_OF_MS), Some(THURSDAY_NIGHT_MS));
        assert_eq!(
            earliest_replacement_kickoff_ms(&roster.starters[0], &roster.bench, AS_OF_MS),
            Some(SUNDAY_LONDON_MS),
            "uncapped, the brief's first reading would have said Sunday"
        );

        roster.starters[0].occupant.as_mut().unwrap().designation =
            Some(crate::roster::Designation::InjuredReserve);
        assert_eq!(lineup_deadline_ms(&roster, AS_OF_MS), Some(THURSDAY_NIGHT_MS), "IR alike");
    }

    /// Once his game has kicked off he is locked in and the slot cannot be
    /// fixed, so there is no deadline left to name — never a past one.
    #[test]
    fn a_starter_already_locked_in_leaves_his_slot_with_no_deadline() {
        let roster: Roster =
            serde_json::from_str(include_str!("../tests/fixtures/roster-starter-locks-first.json"))
                .unwrap();
        assert_eq!(lineup_deadline_ms(&roster, THURSDAY_NIGHT_MS), None);
    }

    /// A slot whose starter kicks off *after* a replacement keeps the
    /// replacement's earlier kickoff: the cap only ever moves a deadline
    /// earlier.
    #[test]
    fn the_cap_never_moves_a_deadline_later() {
        let roster = roster();
        let flex = slot(&roster, "W/R/T");
        assert_eq!(flex.occupant.as_ref().unwrap().kickoff_ms, Some(SUNDAY_1PM_MS));
        assert_eq!(slot_deadline_ms(&flex, &roster.bench, AS_OF_MS), Some(THURSDAY_NIGHT_MS));
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
