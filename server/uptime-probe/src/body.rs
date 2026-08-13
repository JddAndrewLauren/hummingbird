//! The `context_snapshots.payload` this poller writes: ADR-0015's envelope
//! around a `uptime/v1` body, one row per manifest-declared service.
//!
//! **This is one half of a cross-language contract.** The other half is
//! `client/web/src/screens/uptime-pane/uptime.ts`'s parser, and nothing
//! mechanically connects them — the same "no type, no schema, no compiler
//! on either side can see the other" posture `kimi-balance`'s
//! `tests/contract.rs` documents, and this crate's own `tests/contract.rs`
//! asserts the same way.
//!
//! **The raw observation, not a precomputed band.** This body carries
//! exactly what `verdict::decide` was given — the declared expectation and
//! `expect_status`, and what this run actually observed (a status, or an
//! error) — and nothing derived from "now" or from the verdict itself.
//! Whether a service currently reads as divergent is the pane's own
//! read-time judgement, exactly as every other pane's band is
//! (`kimi.ts`'s `kimiBand`, `github.ts`'s `githubBand`) — a stored
//! `"divergent": true` field here would be a second place that judgement
//! could disagree with itself between the moment this poller ran and the
//! moment a reader looks.

use serde::Serialize;

use crate::manifest::{Expected, Service};
use crate::verdict::Outcome;

/// How often this poller says it runs, for `Freshness`'s declared cadence.
/// **Must match `.github/workflows/uptime-probe.yml`'s cron.** Hourly, as
/// one unit across every declared service (ADR-0017 decision 6) — probing
/// the scale-to-zero runner more often would cost real wake-ups to learn
/// nothing new, and there is no per-service override of this: the manifest
/// carries no interval field at all.
pub const POLLED_EVERY_MS: i64 = 60 * 60 * 1000;

fn expected_str(expected: Expected) -> &'static str {
    match expected {
        Expected::On => "on",
        Expected::Off => "off",
    }
}

/// One service's whole reported state — the `uptime/v1` body, written once
/// per declared service under `context_snapshots.key` = the service's own
/// `id`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProbeBody {
    /// `"on"` or `"off"`, copied straight off `services.json` — the pane
    /// reads this rather than re-deriving it, since the manifest is this
    /// poller's own committed intent and the body is its report of what it
    /// found against that intent.
    pub expected: &'static str,
    pub expect_status: u16,
    /// The status this run observed, or `None` when the service was
    /// unreachable at all — mutually exclusive with `error`.
    pub observed_status: Option<u16>,
    /// The transport error's own words, verbatim, or `None` when a status
    /// was observed — carried through rather than collapsed into a closed
    /// vocabulary, so a DNS failure and a timeout read as two different
    /// sentences on the pane (this crate's `verdict` module treats both as
    /// one `Unreachable` case; the words are what keep them distinguishable
    /// to a reader).
    pub error: Option<String>,
}

impl ProbeBody {
    /// Builds the body this run writes for one service, from what it
    /// actually observed.
    pub fn from_outcome(service: &Service, outcome: &Outcome) -> Self {
        match outcome {
            Outcome::Reached(status) => ProbeBody {
                expected: expected_str(service.expected),
                expect_status: service.expect_status,
                observed_status: Some(*status),
                error: None,
            },
            Outcome::Unreachable(reason) => ProbeBody {
                expected: expected_str(service.expected),
                expect_status: service.expect_status,
                observed_status: None,
                error: Some(reason.clone()),
            },
        }
    }

    /// Wraps the body in ADR-0015's envelope — the `payload` value of one
    /// `POST /api/snapshots` call.
    pub fn envelope(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": hummingbird_domain::UPTIME_V1,
            "polled_every_ms": POLLED_EVERY_MS,
            "body": self,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Expected;

    fn service() -> Service {
        Service {
            id: "authority".to_string(),
            url: "https://hb.twinion.net/api/items".to_string(),
            method: "GET".to_string(),
            expect_status: 401,
            expected: Expected::On,
        }
    }

    #[test]
    fn a_reached_outcome_carries_the_status_and_no_error() {
        let body = ProbeBody::from_outcome(&service(), &Outcome::Reached(401));
        assert_eq!(body.expected, "on");
        assert_eq!(body.observed_status, Some(401));
        assert_eq!(body.error, None);
    }

    #[test]
    fn an_unreachable_outcome_carries_the_error_and_no_status() {
        let body = ProbeBody::from_outcome(&service(), &Outcome::Unreachable("timed out".to_string()));
        assert_eq!(body.observed_status, None);
        assert_eq!(body.error, Some("timed out".to_string()));
    }

    /// The envelope half of the contract: `SnapshotEnvelope::parse` is the
    /// exact check `POST /api/snapshots` runs, so anything this poller can
    /// build must survive it.
    #[test]
    fn the_envelope_this_poller_builds_passes_the_authoritys_own_parse() {
        let body = ProbeBody::from_outcome(&service(), &Outcome::Reached(401));
        let payload = body.envelope().to_string();
        let parsed = hummingbird_domain::SnapshotEnvelope::parse(&payload)
            .expect("the authority accepts what this poller writes");
        assert_eq!(parsed.schema, "uptime/v1");
        assert_eq!(parsed.polled_every_ms, Some(POLLED_EVERY_MS));
    }

    /// The unreachable shape must still build a valid envelope — `error` is
    /// `Some` and `observed_status` is `None`, and that is a legitimate
    /// value, not a failure to construct.
    #[test]
    fn an_unreachable_service_still_builds_a_valid_envelope() {
        let body = ProbeBody::from_outcome(&service(), &Outcome::Unreachable("dns error".to_string()));
        let payload = body.envelope().to_string();
        hummingbird_domain::SnapshotEnvelope::parse(&payload)
            .expect("an unreachable-service body still parses");
    }
}
