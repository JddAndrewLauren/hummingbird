//! ADR-0014's live predicate: written once here so the read-time sort, the
//! delivery-warranting check (#139), and the client mirror never re-spell
//! it as three raw column comparisons — each right in isolation, but only
//! their conjunction is the truth about whether an alert still rings.

/// An alert is live when it has not expired and neither its resolution nor
/// its dismissal is still current — where a lifecycle stamp is current
/// only if the alert has not been raised since (ADR-0014):
///
/// ```text
///     (resolved_at  IS NULL OR raised_at  > resolved_at)
/// AND (dismissed_at IS NULL OR raised_at  > dismissed_at)
/// AND (expires_at   IS NULL OR expires_at > now)
/// ```
///
/// This is a comparison, never a clear: `dismissed_at` keeps meaning "the
/// human waved this away at T," so a raise stamped after T is a later
/// occurrence and rings again, while a replay of the very raise that was
/// dismissed is stamped before (or at) T and stays quiet. Same shape for
/// `resolved_at`. `now` matters only for the expiry clause — pass the
/// caller's clock, never a stored value.
pub fn is_live(
    raised_at: i64,
    resolved_at: Option<i64>,
    dismissed_at: Option<i64>,
    expires_at: Option<i64>,
    now: i64,
) -> bool {
    let not_resolved = resolved_at.is_none_or(|resolved_at| raised_at > resolved_at);
    let not_dismissed = dismissed_at.is_none_or(|dismissed_at| raised_at > dismissed_at);
    let not_expired = expires_at.is_none_or(|expires_at| expires_at > now);
    not_resolved && not_dismissed && not_expired
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_alert_with_no_lifecycle_stamps_is_live() {
        assert!(is_live(1000, None, None, None, 5000));
    }

    #[test]
    fn dismissed_at_the_same_or_a_later_stamp_than_raised_at_is_not_live() {
        // Dismissal at T for a raise also at T: the raise is not *after*
        // the dismissal, so it stays quiet.
        assert!(!is_live(1000, None, Some(1000), None, 5000));
        assert!(!is_live(1000, None, Some(2000), None, 5000));
    }

    #[test]
    fn a_raise_stamped_after_the_dismissal_is_live_again() {
        assert!(is_live(3000, None, Some(2000), None, 5000));
    }

    #[test]
    fn resolved_at_behaves_symmetrically_with_dismissed_at() {
        assert!(!is_live(1000, Some(1000), None, None, 5000));
        assert!(!is_live(1000, Some(2000), None, None, 5000));
        assert!(is_live(3000, Some(2000), None, None, 5000));
    }

    #[test]
    fn expiry_is_checked_against_now_not_raised_at() {
        assert!(!is_live(1000, None, None, Some(4000), 5000), "expired before now");
        assert!(is_live(1000, None, None, Some(6000), 5000), "not yet expired");
        assert!(!is_live(1000, None, None, Some(5000), 5000), "expiry is exclusive at now");
    }

    #[test]
    fn all_three_clauses_must_hold() {
        // Live on resolution and dismissal, but expired: not live.
        assert!(!is_live(3000, Some(2000), Some(2000), Some(1000), 5000));
        // Live on resolution and expiry, but still dismissed: not live.
        assert!(!is_live(1000, Some(500), Some(2000), Some(9000), 5000));
    }
}
