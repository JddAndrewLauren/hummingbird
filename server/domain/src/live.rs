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

/// When an alert settled, or `None` while it is still live — the age
/// ADR-0016's wire horizon measures from, written here beside [`is_live`]
/// so the two can never be spelled apart.
///
/// `settled_at(..).is_none()` is exactly `is_live(..)`, by construction: it
/// asks [`is_live`] rather than re-deriving the three clauses, and a test
/// pins the equivalence over `is_live`'s own matrix.
///
/// The answer is the **maximum** of the stamps that are *currently*
/// settling it — `resolved_at`/`dismissed_at` where the alert has not been
/// raised since (ADR-0014), `expires_at` where it has already passed. `min`
/// would be the strict reading of when dormancy began, since [`is_live`] is
/// a conjunction and the earliest settling stamp is where it first turned
/// false. `max` is chosen deliberately as the conservative one: it reads
/// "every applicable stamp is older than the horizon," so a row is carried
/// on the wire for longer rather than shorter, and a fourth lifecycle
/// column added later widens the conjunction without changing this
/// function's meaning.
pub fn settled_at(
    raised_at: i64,
    resolved_at: Option<i64>,
    dismissed_at: Option<i64>,
    expires_at: Option<i64>,
    now: i64,
) -> Option<i64> {
    if is_live(raised_at, resolved_at, dismissed_at, expires_at, now) {
        return None;
    }
    [
        resolved_at.filter(|resolved_at| raised_at <= *resolved_at),
        dismissed_at.filter(|dismissed_at| raised_at <= *dismissed_at),
        expires_at.filter(|expires_at| *expires_at <= now),
    ]
    .into_iter()
    .flatten()
    .max()
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

    /// The whole matrix `is_live` is tested over above, asserted as one
    /// equivalence: this is the guard against ADR-0016's horizon and
    /// ADR-0014's live predicate ever drifting apart.
    #[test]
    fn settled_at_is_none_exactly_when_the_alert_is_live() {
        let stamps = [None, Some(500), Some(1000), Some(2000), Some(9000)];
        for raised_at in [1000, 3000] {
            for resolved_at in stamps {
                for dismissed_at in stamps {
                    for expires_at in stamps {
                        let now = 5000;
                        assert_eq!(
                            settled_at(raised_at, resolved_at, dismissed_at, expires_at, now)
                                .is_none(),
                            is_live(raised_at, resolved_at, dismissed_at, expires_at, now),
                            "raised {raised_at}, resolved {resolved_at:?}, \
                             dismissed {dismissed_at:?}, expires {expires_at:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn settled_at_is_the_latest_stamp_that_still_settles_it() {
        // Dismissed and expired: the later of the two is the answer.
        assert_eq!(
            settled_at(1000, None, Some(2000), Some(4000), 5000),
            Some(4000)
        );
        assert_eq!(
            settled_at(1000, None, Some(4000), Some(2000), 5000),
            Some(4000)
        );
        // All three settling: still the maximum.
        assert_eq!(
            settled_at(1000, Some(3000), Some(2000), Some(4000), 5000),
            Some(4000)
        );
        // A stamp equal to `raised_at` settles it (the raise is not *after*).
        assert_eq!(settled_at(1000, None, Some(1000), None, 5000), Some(1000));
    }

    #[test]
    fn a_stamp_the_alert_was_raised_after_does_not_settle_it() {
        // Re-raised after the dismissal, and expired: only the expiry is
        // still settling it, so the stale dismissal never becomes the age.
        assert_eq!(
            settled_at(3000, None, Some(2000), Some(4000), 5000),
            Some(4000)
        );
        // Re-raised after both — and not expired — is live again.
        assert_eq!(settled_at(3000, Some(2000), Some(2000), Some(9000), 5000), None);
        // An expiry still in the future settles nothing.
        assert_eq!(settled_at(1000, None, Some(2000), Some(9000), 5000), Some(2000));
    }
}
