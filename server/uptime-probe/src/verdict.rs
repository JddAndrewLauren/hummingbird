//! The one pure decision this poller makes: does what this probe just saw
//! agree with what `services.json` declared, or not.
//!
//! **Divergence from the manifest's stated intent is the only thing that
//! lifts a band** (ADR-0017 decision 4) — which makes this function a
//! two-value read, not a five-band judgement. The pane derives its own
//! salience from the raw facts this crate writes (`body.rs`); nothing here
//! computes a `Band` at all, on `github-status`'s own "the verdict, not a
//! precomputed band" reasoning — a stored judgement is a fact that can
//! disagree with itself between the moment this poller ran and the moment a
//! reader looks.

use crate::manifest::Expected;

/// What one HTTP attempt against a declared `url` actually observed.
/// `Unreachable` covers every transport-level failure this poller cannot
/// tell apart mechanically — DNS, TLS, a timeout, a connection refused —
/// and carries the underlying error's own words rather than inventing a
/// closed vocabulary for them, so a DNS failure and a timeout still read as
/// two different sentences on the pane even though this function treats
/// them identically.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Reached(u16),
    Unreachable(String),
}

/// Whether the observed [`Outcome`] agrees with the service's declared
/// [`Expected`] state and `expect_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Quiet agreement — a healthy `expected: "on"` service answering with
    /// its declared status, or an `expected: "off"` service correctly
    /// unreachable (the sweeper's own exclusion is what keeps this arm from
    /// ever meaning "never HTTP-reachable" for a *live* process).
    Agreement,
    /// Divergence from the manifest's stated intent — the only thing that
    /// lifts a band. Covers every one of the acceptance combinations:
    /// expected-on/unreachable, expected-on/wrong-status,
    /// expected-off/reachable (the "something is running that should not"
    /// case, ADR-0017 decision 4's own phrasing).
    Divergent,
}

/// Decides one service's verdict from its declared expectation and status,
/// and what this run actually observed.
pub fn decide(expected: Expected, expect_status: u16, outcome: &Outcome) -> Verdict {
    match (expected, outcome) {
        (Expected::On, Outcome::Reached(status)) if *status == expect_status => Verdict::Agreement,
        (Expected::On, Outcome::Reached(_)) => Verdict::Divergent,
        (Expected::On, Outcome::Unreachable(_)) => Verdict::Divergent,
        (Expected::Off, Outcome::Unreachable(_)) => Verdict::Agreement,
        (Expected::Off, Outcome::Reached(_)) => Verdict::Divergent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The acceptance line's own six combinations, verbatim.

    #[test]
    fn expected_on_and_reached_the_declared_status_is_agreement() {
        assert_eq!(decide(Expected::On, 401, &Outcome::Reached(401)), Verdict::Agreement);
    }

    #[test]
    fn expected_on_and_unreachable_is_divergent() {
        assert_eq!(
            decide(Expected::On, 401, &Outcome::Unreachable("connection refused".to_string())),
            Verdict::Divergent
        );
    }

    /// The agreement the sweeper's own exclusion protects: a service the
    /// manifest declares deliberately down, correctly not answering, must
    /// read as quiet agreement rather than permanently red.
    #[test]
    fn expected_off_and_unreachable_is_agreement() {
        assert_eq!(
            decide(Expected::Off, 200, &Outcome::Unreachable("timed out".to_string())),
            Verdict::Agreement
        );
    }

    /// The fault the other way: something is running that should not be.
    #[test]
    fn expected_off_and_reached_is_divergent_even_at_the_declared_status() {
        assert_eq!(decide(Expected::Off, 200, &Outcome::Reached(200)), Verdict::Divergent);
    }

    #[test]
    fn a_wrong_status_code_is_divergent() {
        assert_eq!(decide(Expected::On, 401, &Outcome::Reached(500)), Verdict::Divergent);
    }

    /// A transport error is read no differently from any other
    /// unreachability — this function does not distinguish a DNS failure
    /// from a timeout, on this module's own header (the pane's rendered
    /// text is what carries the distinction, not this decision).
    #[test]
    fn a_transport_error_is_divergent_when_expected_on() {
        assert_eq!(
            decide(Expected::On, 200, &Outcome::Unreachable("dns error: NXDOMAIN".to_string())),
            Verdict::Divergent
        );
    }
}
