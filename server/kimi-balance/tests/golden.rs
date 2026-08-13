//! **The golden envelope, pinned byte-for-byte** — `race-poll/tests/golden.rs`'s
//! own move: the exact `context_snapshots.payload` this poller would write
//! from a real Moonshot response, committed so a shape change is a decision
//! taken deliberately rather than a silent drift `cargo test` never catches.
//!
//! Two fixtures, for the two facts the ADR names explicitly: an ordinary
//! reading, and one where `cash_balance` has gone negative while
//! `available_balance` stays positive — the case the pane must not collapse
//! into a single misleading number.
//!
//! **Regenerating a golden file to make this test pass is the one thing
//! that defeats it.** A diff here is a decision about the wire contract
//! `kimi.ts` parses, not a chore.

use hummingbird_kimi_balance::balance;
use hummingbird_kimi_balance::body::KimiBalanceBody;

const RESPONSE_OK: &str = include_str!("fixtures/balance-ok.json");
const RESPONSE_NEGATIVE_CASH: &str = include_str!("fixtures/balance-negative-cash.json");
const RESPONSE_API_ERROR: &str = include_str!("fixtures/balance-api-error.json");
const RESPONSE_MALFORMED: &str = include_str!("fixtures/balance-malformed.json");

const GOLDEN_OK: &str = include_str!("fixtures/golden-body-ok.json");
const GOLDEN_NEGATIVE_CASH: &str = include_str!("fixtures/golden-body-negative-cash.json");

fn rendered_envelope(response: &str) -> String {
    let balance = balance::parse(response).expect("the fixture parses");
    let mut rendered =
        serde_json::to_string_pretty(&KimiBalanceBody::from_balance(balance).envelope())
            .expect("the envelope serializes");
    rendered.push('\n');
    rendered
}

#[test]
fn the_envelope_this_poller_writes_for_an_ordinary_balance_is_the_golden_body_byte_for_byte() {
    assert_eq!(
        rendered_envelope(RESPONSE_OK),
        GOLDEN_OK,
        "the ordinary-balance body has moved away from \
         tests/fixtures/golden-body-ok.json — this is kimi.ts's parser \
         contract, not a snapshot to regenerate"
    );
}

#[test]
fn the_envelope_this_poller_writes_for_a_negative_cash_balance_is_the_golden_body_byte_for_byte() {
    assert_eq!(
        rendered_envelope(RESPONSE_NEGATIVE_CASH),
        GOLDEN_NEGATIVE_CASH,
        "the negative-cash body has moved away from \
         tests/fixtures/golden-body-negative-cash.json — the one case \
         the ADR names explicitly, where a positive available_balance \
         hides a negative cash position"
    );
}

/// `code != 0` (`exceeded_current_quota_error`'s own shape) must refuse
/// before ever reaching the body — there is no golden file for this case
/// because no envelope is ever built from it.
#[test]
fn a_nonzero_code_response_never_produces_an_envelope() {
    assert!(balance::parse(RESPONSE_API_ERROR).is_err());
}

/// A response that is not the expected shape at all refuses the same way.
#[test]
fn a_malformed_response_never_produces_an_envelope() {
    assert!(balance::parse(RESPONSE_MALFORMED).is_err());
}
