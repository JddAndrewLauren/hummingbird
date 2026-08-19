//! The `context_snapshots.payload` this poller writes: ADR-0015's envelope
//! around a `kimi-balance/v1` body.
//!
//! **This is one half of a cross-language contract.** The other half is
//! `client/core/src/decisions/panes/kimi.rs`'s `parse_kimi_body` (sunk out
//! of `client/web/src/screens/kimi-pane/kimi.ts` at #534), and nothing
//! mechanically connects them — the body inside the envelope is deliberately
//! unfrozen and opaque to the server. `tests/contract.rs`'s
//! `the_body_this_poller_writes_is_the_body_the_pane_parses` is the only
//! guard against the two drifting, on `city-waste`'s own reasoning: it
//! asserts the literal snake_case key names against that Rust module's own
//! text rather than going through this module's serde.
//!
//! The body is exactly the three numbers Moonshot's endpoint reports —
//! `available_balance`, `voucher_balance`, `cash_balance` — and nothing
//! derived from them. ADR-0017 decision 5 is explicit that this pane reports
//! the gauge directly rather than inferring a burn rate from noisy deltas,
//! so there is no `runway_days` or similar field here for a later drift to
//! disagree with: the pane derives its own band from `available_balance`
//! alone (`kimi.ts`).

use serde::Serialize;

/// How often this poller says it runs, for `Freshness`'s declared cadence.
/// **Must match `.github/workflows/kimi-balance.yml`'s cron.** Six hours,
/// not daily — a balance is a countdown to a hard cliff, and polling slower
/// than half the shortest plausible runway produces a warning that arrives
/// after the outage (the same reasoning `race-poll`'s 6h season refresh
/// documents for its own lane).
pub const POLLED_EVERY_MS: i64 = 6 * 60 * 60 * 1000;

/// The one `context_snapshots.key` this source owns — the whole answer is
/// one row, replaced wholesale by each poll, on the gauge doctrine ADR-0009
/// already states for every snapshot row.
pub const SNAPSHOT_KEY: &str = "balance";

/// The `kimi-balance/v1` body. Field names are the wire contract; see the
/// module note above before renaming one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct KimiBalanceBody {
    pub available_balance: f64,
    pub voucher_balance: f64,
    pub cash_balance: f64,
}

impl KimiBalanceBody {
    pub fn from_balance(balance: crate::balance::Balance) -> KimiBalanceBody {
        KimiBalanceBody {
            available_balance: balance.available_balance,
            voucher_balance: balance.voucher_balance,
            cash_balance: balance.cash_balance,
        }
    }

    /// Wraps the body in ADR-0015's envelope — the `payload` value of a
    /// `POST /api/snapshots`.
    pub fn envelope(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": hummingbird_domain::KIMI_BALANCE_V1,
            "polled_every_ms": POLLED_EVERY_MS,
            "body": self,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::balance::Balance;

    #[test]
    fn the_body_carries_the_balance_verbatim() {
        let balance = Balance { available_balance: 12.4, voucher_balance: 8.0, cash_balance: 4.4 };
        let body = KimiBalanceBody::from_balance(balance);
        assert_eq!(body.available_balance, 12.4);
        assert_eq!(body.voucher_balance, 8.0);
        assert_eq!(body.cash_balance, 4.4);
    }

    /// The envelope half of the contract: `SnapshotEnvelope::parse` is the
    /// exact check `POST /api/snapshots` runs, so anything this poller can
    /// build must survive it.
    #[test]
    fn the_envelope_this_poller_builds_passes_the_authoritys_own_parse() {
        let body = KimiBalanceBody::from_balance(Balance {
            available_balance: 4.10,
            voucher_balance: 5.10,
            cash_balance: -1.0,
        });
        let payload = body.envelope().to_string();
        let parsed = hummingbird_domain::SnapshotEnvelope::parse(&payload)
            .expect("the authority accepts what this poller writes");
        assert_eq!(parsed.schema, "kimi-balance/v1");
        assert_eq!(parsed.polled_every_ms, Some(POLLED_EVERY_MS));
    }
}
