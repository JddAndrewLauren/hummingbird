//! **The cross-language contract, and the only guard against it drifting.**
//!
//! The body inside ADR-0015's envelope is deliberately unfrozen and opaque
//! to the server: `SnapshotEnvelope` carries it through as text, `POST
//! /api/snapshots` never looks inside it, and the pane's own parser is what
//! pins its shape. So this file asserts the literal snake_case key names
//! twice — once on the JSON this crate actually produces, and once against
//! the text of the code that consumes it — on `city-waste/tests/
//! contract.rs`'s own reasoning: **no type, no schema and no compiler on
//! either side can see the other**, and a rename made on only one side
//! would otherwise still pass every test in both languages while the pane
//! silently read "no balance has been fetched yet" forever.
//!
//! **Retargeted at #534.** ADR-0025/#534 sank the pane's parser out of
//! `client/web/src/screens/kimi-pane/kimi.ts` and into
//! `client/core/src/decisions/panes/kimi.rs::parse_kimi_body` — the real
//! parse surface now lives there (`kimi.ts` kept its name but is now a thin
//! rendering wrapper over the seam, and no longer spells `body.…` anywhere).

use hummingbird_kimi_balance::balance::Balance;
use hummingbird_kimi_balance::body::{KimiBalanceBody, POLLED_EVERY_MS, SNAPSHOT_KEY};

/// Every key the pane reads out of the body, spelled exactly as it appears
/// on the wire.
const KEYS_THE_PANE_READS: &[&str] = &["available_balance", "voucher_balance", "cash_balance"];

const KIMI_RS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../client/core/src/decisions/panes/kimi.rs"
));

fn sample() -> KimiBalanceBody {
    KimiBalanceBody::from_balance(Balance {
        available_balance: 4.10,
        voucher_balance: 5.10,
        cash_balance: -1.0,
    })
}

#[test]
fn the_body_this_poller_writes_is_the_body_the_pane_parses() {
    let payload = sample().envelope();
    let body = payload.get("body").expect("the envelope carries a body");
    let object = body.as_object().expect("the body is an object");

    for key in KEYS_THE_PANE_READS {
        assert!(
            object.contains_key(*key),
            "`{key}` is gone from the body this poller writes — `parse_kimi_body` \
             would answer a gap and the pane would read 'not fetched yet' forever"
        );
        assert!(
            KIMI_RS.contains(&format!("number(\"{key}\")")),
            "`number(\"{key}\")` is gone from kimi.rs's parser — `{key}` is written \
             here but no longer read there, one side was renamed alone"
        );
    }

    // Values, not just names, including the negative-cash case the ADR
    // names explicitly.
    assert_eq!(object["available_balance"], 4.10);
    assert_eq!(object["voucher_balance"], 5.10);
    assert_eq!(object["cash_balance"], -1.0);
}

/// The two constants that have to agree across the seam, checked against
/// the TypeScript's own literals: the source string (also the envelope's
/// `schema`, which the pane refuses anything else against) and the snapshot
/// key.
#[test]
fn the_source_and_snapshot_key_agree_with_the_pane() {
    assert!(
        KIMI_RS.contains(r#"pub const SOURCE: &str = "kimi-balance/v1";"#),
        "the pane reads a different source than this poller writes"
    );
    assert!(
        KIMI_RS.contains(r#"pub const SNAPSHOT_KEY: &str = "balance";"#),
        "the pane reads a different snapshot key than this poller writes"
    );
    assert_eq!(hummingbird_domain::KIMI_BALANCE_V1, "kimi-balance/v1");
    assert_eq!(SNAPSHOT_KEY, "balance");
}

/// `.github/workflows/kimi-balance.yml`'s cron must agree with this — the
/// declared cadence `Freshness` reads.
#[test]
fn polled_every_ms_is_six_hours() {
    assert_eq!(POLLED_EVERY_MS, 6 * 60 * 60 * 1000);
}
