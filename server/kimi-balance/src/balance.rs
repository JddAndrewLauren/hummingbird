//! Parsing `GET {host}/v1/users/me/balance`'s response body — the one thing
//! this poller reads over the network besides the authority itself.
//!
//! ```text
//! { "code": 0, "data": { "available_balance": 12.4,
//!                        "voucher_balance": 8.0,
//!                        "cash_balance": 4.4 },
//!   "scode": "0x0", "status": true }
//! ```
//!
//! **`code != 0` is a refused answer, not a malformed one, and it is not
//! written.** The brief's own scenario is `exceeded_current_quota_error`:
//! Moonshot returns a non-zero `code` on every call once
//! `available_balance <= 0`, so a `code != 0` response is exactly the
//! moment a stale-but-honest "$0.00, exhausted" answer already on the pane
//! is more useful than whatever this endpoint's error body would say in its
//! place — the same "fail loudly, write nothing" posture `race-poll`
//! documents for a feed shape it does not recognise.
//!
//! `data` is only read once `code == 0`, so a response that omits it
//! alongside a non-zero code is not a second failure mode to invent words
//! for.

use serde::Deserialize;

/// The three numbers this poller reports, read straight off `data`. Kept as
/// `f64` all the way to [`crate::body::KimiBalanceBody`] — Moonshot's own
/// wire format is a JSON number with cents, and there is no fixed-point
/// currency type anywhere else in this crate to round-trip through.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Balance {
    pub available_balance: f64,
    pub voucher_balance: f64,
    pub cash_balance: f64,
}

/// Every way this response could fail to be read, in words a operator's log
/// (never a reader's screen — this poller has no pane of its own) can use.
#[derive(Debug, Clone, PartialEq)]
pub enum BalanceError {
    /// The body is not the JSON shape this poller expects at all — not even
    /// well-formed enough to have a `code`.
    Malformed(String),
    /// A well-formed envelope, but `code != 0` — Moonshot's own signal that
    /// the call did not succeed (the balance endpoint's own analogue of
    /// `exceeded_current_quota_error`). `scode` is carried through verbatim
    /// since it is the more specific of the two the API returns.
    ApiError { code: i64, scode: String },
}

impl std::fmt::Display for BalanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BalanceError::Malformed(reason) => write!(f, "malformed balance response: {reason}"),
            BalanceError::ApiError { code, scode } => {
                write!(f, "balance endpoint returned code {code} ({scode})")
            }
        }
    }
}

#[derive(Deserialize)]
struct RawEnvelope {
    code: i64,
    #[serde(default)]
    scode: String,
    data: Option<RawData>,
}

#[derive(Deserialize)]
struct RawData {
    available_balance: f64,
    voucher_balance: f64,
    cash_balance: f64,
}

/// Reads the balance endpoint's response body, or the reason it could not.
pub fn parse(body: &str) -> Result<Balance, BalanceError> {
    let envelope: RawEnvelope =
        serde_json::from_str(body).map_err(|e| BalanceError::Malformed(e.to_string()))?;

    if envelope.code != 0 {
        return Err(BalanceError::ApiError { code: envelope.code, scode: envelope.scode });
    }

    let data = envelope
        .data
        .ok_or_else(|| BalanceError::Malformed("`code` is 0 but `data` is missing".to_string()))?;

    Ok(Balance {
        available_balance: data.available_balance,
        voucher_balance: data.voucher_balance,
        cash_balance: data.cash_balance,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../tests/fixtures/balance-ok.json");
    const NEGATIVE_CASH: &str = include_str!("../tests/fixtures/balance-negative-cash.json");
    const API_ERROR: &str = include_str!("../tests/fixtures/balance-api-error.json");
    const MALFORMED: &str = include_str!("../tests/fixtures/balance-malformed.json");

    #[test]
    fn reads_an_ordinary_response() {
        assert_eq!(
            parse(OK).unwrap(),
            Balance { available_balance: 12.4, voucher_balance: 8.0, cash_balance: 4.4 }
        );
    }

    /// The fact the ADR names explicitly: a positive `available_balance` can
    /// hide a negative `cash_balance` — the account owes, even though the
    /// endpoint's headline number is still positive.
    #[test]
    fn reads_a_negative_cash_balance_without_rejecting_it() {
        let balance = parse(NEGATIVE_CASH).unwrap();
        assert!(balance.cash_balance < 0.0, "the fixture must actually exercise this");
        assert_eq!(
            balance,
            Balance { available_balance: 4.10, voucher_balance: 5.10, cash_balance: -1.0 }
        );
    }

    /// `code != 0` — the account is out of quota — is a refusal, not a
    /// malformed body: it must be distinguishable from JSON that failed to
    /// parse at all, since only one of the two is ever plausibly transient.
    #[test]
    fn a_nonzero_code_is_an_api_error_not_a_malformed_body() {
        assert_eq!(
            parse(API_ERROR),
            Err(BalanceError::ApiError {
                code: 43_001,
                scode: "0xa10a1".to_string(),
            })
        );
    }

    #[test]
    fn a_body_that_is_not_the_expected_shape_is_malformed() {
        assert!(matches!(parse(MALFORMED), Err(BalanceError::Malformed(_))));
        assert!(matches!(parse("not json at all"), Err(BalanceError::Malformed(_))));
        assert!(matches!(parse("{}"), Err(BalanceError::Malformed(_))));
    }

    /// `code == 0` with `data` missing is refused as malformed rather than
    /// silently defaulting every field to zero — a fabricated "$0.00" is a
    /// wrong answer, not an honest empty one.
    #[test]
    fn code_zero_with_no_data_is_malformed_not_a_fabricated_zero_balance() {
        assert!(matches!(
            parse(r#"{"code":0,"scode":"0x0","status":true}"#),
            Err(BalanceError::Malformed(_))
        ));
    }
}
