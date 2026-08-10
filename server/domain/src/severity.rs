//! ADR-0014's severity ratchet: `alerts.severity` and `rules.severity` are
//! free text with no `CHECK` in the DDL — a source or a rule author may
//! stamp anything — so "does this severity outrank that one" needs a total
//! order over arbitrary strings, written once. It decides the alerts
//! ingest handler's ratchet-while-live (a mint against a live alert may
//! raise severity, never lower it), and it is meant to double as the order
//! the rules-UI severity dropdown (#140) presents, so the two surfaces
//! never disagree.

/// The known severities, lowest to highest.
pub const SEVERITIES: [&str; 4] = ["low", "normal", "high", "urgent"];

/// [`SEVERITIES`]'s index, one-based. A string outside the list — free
/// text the vocabulary hasn't caught up with, or simply absent — ranks
/// `0`: below every known severity, deliberately. An unranked value must
/// never win a ratchet it did not earn (ADR-0014's "not an accidental top
/// rank"), and it never panics on unrecognised input.
pub fn severity_rank(severity: &str) -> usize {
    SEVERITIES.iter().position(|&s| s == severity).map_or(0, |i| i + 1)
}

/// The higher-ranked of `current` and `incoming`, by [`severity_rank`].
/// Ties — equal known ranks, or both unranked (including both absent) —
/// favour `current`, so a repeat, a lateral move, or a source that stops
/// sending severity while an alert is still live never displaces a real
/// stored value. `None` ranks alongside the unranked strings.
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
