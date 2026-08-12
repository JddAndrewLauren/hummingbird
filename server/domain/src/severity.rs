//! The one severity order. `alerts.severity` and `rules.severity` are free
//! text with no `CHECK` in the DDL — a source or a rule author may stamp
//! anything — so "does this severity outrank that one" needs a total order
//! over arbitrary strings, written once.
//!
//! Three surfaces read it, and they must never disagree (ADR-0014, as
//! amended 2026-08-12 by #188):
//!
//! 1. **The mint fold** — N rules matching one event mint *one* alert at the
//!    highest severity among them ([`higher_severity`] folded over the
//!    verdicts, order-independently, before any write). `sweep::tick` and
//!    each evaluated-stream poller do this themselves.
//! 2. **The ring gate** — `authority::delivery` rings only above the highest
//!    severity already rung for an alert, rule and generation. This, not the
//!    row, is where severity monotonicity lives.
//! 3. **The rules-UI dropdown** (#140), which presents the same order.
//!
//! Notably *not* a fourth surface: the alerts ingest handler, which sets
//! `severity` absolutely like every other source-owned field. It used to
//! ratchet the stored value while the row was live; #188 moved that to (2).

/// The known severities, lowest to highest.
pub const SEVERITIES: [&str; 4] = ["low", "normal", "high", "urgent"];

/// [`SEVERITIES`]'s index, one-based. A string outside the list — free
/// text the vocabulary hasn't caught up with, or simply absent — ranks
/// `0`: below every known severity, deliberately. An unranked value must
/// never win a fold, or a ring, it did not earn (ADR-0014's "not an
/// accidental top rank"), and it never panics on unrecognised input.
pub fn severity_rank(severity: &str) -> usize {
    SEVERITIES.iter().position(|&s| s == severity).map_or(0, |i| i + 1)
}

/// The higher-ranked of `current` and `incoming`, by [`severity_rank`].
/// Ties — equal known ranks, or both unranked (including both absent) —
/// favour `current`, so folding this over a set of verdicts in any order
/// converges on the same maximum, which is what makes the mint fold
/// order-independent. `None` ranks alongside the unranked strings.
///
/// This is the *fold* over concurrent judgments, never a comparison against
/// a stored row: since #188 no caller passes it a persisted `severity`.
pub fn higher_severity<'a>(current: Option<&'a str>, incoming: Option<&'a str>) -> Option<&'a str> {
    let current_rank = current.map_or(0, severity_rank);
    let incoming_rank = incoming.map_or(0, severity_rank);
    if incoming_rank > current_rank {
        incoming
    } else {
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_severities_rank_in_the_documented_order() {
        let ranks: Vec<usize> = SEVERITIES.iter().map(|s| severity_rank(s)).collect();
        assert_eq!(ranks, vec![1, 2, 3, 4]);
    }

    #[test]
    fn an_unranked_severity_ranks_zero_without_panicking() {
        assert_eq!(severity_rank("bogus"), 0);
        assert_eq!(severity_rank(""), 0);
    }

    #[test]
    fn higher_severity_picks_the_higher_known_rank_either_direction() {
        assert_eq!(higher_severity(Some("urgent"), Some("normal")), Some("urgent"));
        assert_eq!(higher_severity(Some("normal"), Some("urgent")), Some("urgent"));
    }

    #[test]
    fn a_tie_favours_current() {
        assert_eq!(higher_severity(Some("high"), Some("high")), Some("high"));
    }

    #[test]
    fn an_unranked_challenger_never_displaces_a_known_current() {
        assert_eq!(
            higher_severity(Some("normal"), Some("bogus")),
            Some("normal"),
            "an unranked incoming must not win — not even an accidental top rank"
        );
    }

    #[test]
    fn a_known_incoming_beats_an_unranked_current() {
        assert_eq!(higher_severity(Some("bogus"), Some("normal")), Some("normal"));
    }

    #[test]
    fn absent_severities_rank_with_the_unranked() {
        assert_eq!(higher_severity(Some("normal"), None), Some("normal"));
        assert_eq!(higher_severity(None, Some("normal")), Some("normal"));
        assert_eq!(higher_severity(None, None), None);
    }
}
