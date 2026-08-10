//! The write error taxonomy (#101): what the outbound queue (#102) decides
//! on. Ported from `sweep.py`'s terminal-vs-transient reasoning
//! (`docs/sweeper.md`, `_is_terminal`) rather than invented fresh — a 5xx is
//! never terminal whatever its body says, because the server failing to
//! answer says nothing about the write's content and the next attempt might
//! well succeed.
//!
//! Five branches, driven entirely by HTTP status (never by body content —
//! there is no per-field validation-message parsing the way the sweeper
//! parses Linear's `property` field, because this adapter does not need to
//! decide anything finer than "retry or don't"):
//! - **retryable**: network failure, 429, or any 5xx.
//! - **conflict**: 409 — carries the current entity, handled by
//!   [`super::rebase`], never surfaced as a bare status.
//! - **contention**: a second 409, still genuinely disjoint after the one
//!   bounded rebase retry — not a field collision (that stays `Conflict`),
//!   but repeated churn this adapter gives up reissuing against. Carries
//!   the current entity so the dead-letter journal has material, never an
//!   empty field list masquerading as a conflict (#163).
//! - **unauthorized**: 401 — the credential itself is bad, never permanent
//!   (a fresh token can still succeed).
//! - **permanent**: any other 4xx — no retry can fix it.
//! - **already-exists / success**: 2xx — not an error at all, see
//!   [`super::adapter::CreateOutcome`].

use std::fmt;

use serde_json::Value;

/// What the outbound queue (#102) decides on. Every branch except
/// `Conflict` carries no further structure than the queue needs — it is the
/// queue's job to decide an entry's fate, not this adapter's.
#[derive(Debug, Clone, PartialEq)]
pub enum WriteError {
    /// Network, 429, or 5xx: transient, worth retrying unchanged.
    Retryable(String),
    /// Any other 4xx: no retry can fix it.
    Permanent(String),
    /// 401: the credential itself is bad. Never permanent — a fresh token
    /// can still succeed.
    Unauthorized,
    /// 409 that did not auto-resolve: `fields` names every touched field
    /// where the intervening write set something other than what this
    /// write intended — the conflict journal's material. Empty only when
    /// the 409 could not be attributed to specific fields at all (e.g. a
    /// create, which carries no `expected_version` to rebase against).
    Conflict { fields: Vec<String>, current: Value },
    /// A second 409 that is still genuinely disjoint after the one bounded
    /// rebase retry (#163): not a field collision — no field name to give
    /// the conflict journal — just repeated churn. Carries the current
    /// entity so the dead-letter journal still has material to show.
    Contention { current: Value },
    /// The body was not the JSON shape the status promised.
    InvalidResponse(String),
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WriteError::Retryable(message) => write!(f, "retryable: {message}"),
            WriteError::Permanent(message) => write!(f, "permanent: {message}"),
            WriteError::Unauthorized => write!(f, "unauthorized"),
            WriteError::Conflict { fields, .. } => {
                write!(f, "conflict on field(s): {}", fields.join(", "))
            }
            WriteError::Contention { .. } => write!(f, "contention"),
            WriteError::InvalidResponse(message) => write!(f, "invalid response: {message}"),
        }
    }
}

impl std::error::Error for WriteError {}

/// Classifies a non-2xx, non-409 status. 409 is handled separately by the
/// caller, which needs the body to rebase — by the time this runs, the
/// status is known not to be a success and not to be a conflict.
pub fn classify_status(status: u16) -> WriteError {
    match status {
        401 => WriteError::Unauthorized,
        429 => WriteError::Retryable(format!("HTTP {status}")),
        500..=599 => WriteError::Retryable(format!("HTTP {status}")),
        _ => WriteError::Permanent(format!("HTTP {status}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_401_is_unauthorized_never_permanent() {
        assert_eq!(classify_status(401), WriteError::Unauthorized);
    }

    #[test]
    fn a_429_is_retryable() {
        assert!(matches!(classify_status(429), WriteError::Retryable(_)));
    }

    #[test]
    fn every_5xx_is_retryable_whatever_the_code() {
        for status in [500, 502, 503, 599] {
            assert!(
                matches!(classify_status(status), WriteError::Retryable(_)),
                "status {status} should be retryable"
            );
        }
    }

    #[test]
    fn a_plain_400_is_permanent() {
        assert!(matches!(classify_status(400), WriteError::Permanent(_)));
    }

    #[test]
    fn a_403_is_permanent_not_unauthorized() {
        // A device token never sees 403 on this surface (auth::permitted
        // grants Device everything but alert ingest), but if the authority
        // ever did send one, it must not be confused with a dead credential
        // — 403 means the token is fine and the scope is wrong, never a
        // signal to re-prompt.
        assert!(matches!(classify_status(403), WriteError::Permanent(_)));
    }

    #[test]
    fn a_404_is_permanent() {
        assert!(matches!(classify_status(404), WriteError::Permanent(_)));
    }
}
