//! **The Kimi balance question** (#313, ADR-0017 decision 5), sunk out of
//! `screens/kimi-pane/kimi.ts` by ADR-0025 (#534).
//!
//! There is no per-device setup here, unlike `waste.rs`'s bound page: the
//! credential and the host both live in the poller's own Actions
//! secrets/variables, never a `settings` row a device configures. This
//! question therefore has no `unbound` state at all — it is either
//! never-polled (`bound-but-unacquired`) or answered.
//!
//! **Band from the balance itself, never from `cash_balance` alone.**
//! `cash_balance` can go negative while `available_balance` — the number
//! Moonshot actually enforces `exceeded_current_quota_error` against — stays
//! positive, so banding on the sub-field would read a healthy account as
//! exhausted or vice versa.
//!
//! What is here is the *deciding* half: the payload parser that pins the
//! `kimi-balance/v1` body shape, the band and its two thresholds (ADR-0015
//! keeps those two together, and ADR-0025 moves the pair), and the gap
//! **kinds**. What is not here is every word: [`KimiGap`] is a structured
//! enum, not a sentence — "The balance payload isn't JSON", "Update the
//! app.", and `formatUsd`/`kimiCollapsedHeadline`/`kimiGlyph` all stay
//! per-client, on `waste.rs`'s own "gap kinds sink, words do not" rule.
//! `withinBand` is always `None`: this pane has exactly one gauge and no
//! history to derive a time-of-cliff estimate from (ADR-0017 decision 5
//! rejects inferring a burn rate from noisy deltas), and there is only ever
//! one subject for this question, so there is nothing to tie-break against.

use serde::{Deserialize, Serialize};

use super::contract::{AnswerState, Band, PaneAnswerCore};
use super::inputs::{FreshnessFact, PaneEnvelopeFacts, PaneInputs, PaneSnapshotFacts};

/// Both the snapshot's source and, per ADR-0017 decision 5, the only key
/// `server/domain/src/sources.rs` enrols as `Writes::Snapshots` for it — so
/// no alert ever carries it. Nothing here checks that registry (ADR-0015
/// forbids checking a snapshot's `schema` against one), so this constant is
/// this module's own and the two agree by review — `waste.rs`'s own note,
/// verbatim.
pub const SOURCE: &str = "kimi-balance/v1";

/// The one `context_snapshots.key` this question reads, and its subject
/// key — the sentinel subject this question always returns, polled or not.
pub const SNAPSHOT_KEY: &str = "balance";

/// ~13h against a 6h cadence: two missed polls means the countdown has gone
/// unobserved for half a day. The threshold is set by the cost of a wrong
/// answer, not as a bare `2 ×` of the interval — the same reasoning
/// `waste.rs` gives 26h against a daily cadence rather than 48h.
pub const STALE_AFTER_MS: i64 = 13 * 60 * 60 * 1000;

/// Below this many dollars, the very next call could plausibly trip
/// `exceeded_current_quota_error` — the cliff is imminent.
pub const IMMINENT_THRESHOLD_USD: f64 = 1.0;

/// Below this many dollars there is still a real cushion, but not much of
/// one — worth naming as "running low" rather than silence.
pub const NEAR_THRESHOLD_USD: f64 = 5.0;

/// The `kimi-balance/v1` payload body — this module's parser is what pins
/// the shape, since the body inside ADR-0015's envelope is deliberately
/// unfrozen and opaque to everything else. Field names mirror the wire
/// exactly: `available_balance`/`voucher_balance`/`cash_balance`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KimiBody {
    pub available_balance: f64,
    pub voucher_balance: f64,
    pub cash_balance: f64,
}

/// Why this pane has no answer — a **kind**, not a sentence.
///
/// Every arm names what was wrong so a client can say it in its own words —
/// `waste.rs`'s [`super::waste::WasteGap`] discipline, applied here.
/// [`KimiGap::UnknownSchema`] stays separate from [`KimiGap::Malformed`] for
/// the same reason: a newer build wrote a shape this one has never heard of
/// is a fact about *this device*, fixed by updating it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "gap", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum KimiGap {
    /// No snapshot row at all: nothing has ever been fetched.
    NotFetched,
    /// The envelope itself could not be read. `reason` is
    /// `hummingbird_domain::EnvelopeProblem`'s own wording, passed through
    /// as data.
    Malformed { reason: String },
    UnknownSchema { schema: String },
    NotJson,
    NotAnObject,
    /// One or more of `available_balance`/`voucher_balance`/`cash_balance`
    /// is missing or not a finite number. A negative `cash_balance` is
    /// *not* this gap — it is read straight through, per the module header.
    BadNumbers,
}

/// Everything an answered pane needs, decided once and read by both the
/// answer and the client's expanded rendering. **No rendered sentence
/// crosses** — a client composes its headline from these facts.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KimiFacts {
    pub available_balance: f64,
    pub voucher_balance: f64,
    pub cash_balance: f64,
    pub stale: bool,
    pub freshness: FreshnessFact,
}

/// The whole answered fact set, or the reason there is none. One type, so
/// the decision to be a gap and the gap's kind cannot disagree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum KimiResolved {
    Facts(KimiFacts),
    Gap { gap: KimiGap },
}

/// Reads one snapshot row into a body, or says why it could not.
pub fn parse_kimi_body(snapshot: Option<&PaneSnapshotFacts>) -> Result<KimiBody, KimiGap> {
    let Some(snapshot) = snapshot else {
        return Err(KimiGap::NotFetched);
    };
    let (schema, body) = match &snapshot.envelope {
        PaneEnvelopeFacts::Malformed { reason } => {
            return Err(KimiGap::Malformed { reason: reason.clone() })
        }
        PaneEnvelopeFacts::Ok { schema, body } => (schema, body),
    };
    if schema != SOURCE {
        return Err(KimiGap::UnknownSchema { schema: schema.clone() });
    }

    let parsed: serde_json::Value =
        serde_json::from_str(body).map_err(|_| KimiGap::NotJson)?;
    let object = parsed.as_object().ok_or(KimiGap::NotAnObject)?;

    let number = |field: &str| -> Option<f64> {
        object.get(field).and_then(|v| v.as_f64()).filter(|v| v.is_finite())
    };
    let (Some(available_balance), Some(voucher_balance), Some(cash_balance)) =
        (number("available_balance"), number("voucher_balance"), number("cash_balance"))
    else {
        return Err(KimiGap::BadNumbers);
    };

    Ok(KimiBody { available_balance, voucher_balance, cash_balance })
}

/// The whole answered fact set, or the reason there is none.
pub fn kimi_facts(inputs: &PaneInputs) -> KimiResolved {
    let snapshot = inputs.snapshot(SOURCE, SNAPSHOT_KEY);
    let body = match parse_kimi_body(snapshot) {
        Ok(body) => body,
        Err(gap) => return KimiResolved::Gap { gap },
    };
    // `parse_kimi_body` returning `Ok` guarantees the row exists.
    let freshness = snapshot.expect("a parsed body implies a snapshot row").freshness;

    KimiResolved::Facts(KimiFacts {
        available_balance: body.available_balance,
        voucher_balance: body.voucher_balance,
        cash_balance: body.cash_balance,
        stale: freshness.is_stale_beyond(STALE_AFTER_MS),
        freshness,
    })
}

/// How close `available_balance` is to the `$0` cliff — never derived from
/// `cash_balance`, which can be negative while the account is nowhere near
/// exhausted (a voucher balance still covers it).
pub fn kimi_band(available_balance: f64) -> Band {
    if available_balance <= 0.0 {
        Band::Live
    } else if available_balance < IMMINENT_THRESHOLD_USD {
        Band::Imminent
    } else if available_balance < NEAR_THRESHOLD_USD {
        Band::Near
    } else {
        Band::Dormant
    }
}

/// This question's answer for the shell (#313 over #245/ADR-0017), minus its
/// rendering half.
///
/// `within_band` is `None` in every case — see the module header. There is
/// no `unbound` state: this question has no per-device binding at all, so a
/// gap is always [`AnswerState::BoundButUnacquired`].
pub fn kimi_answer(inputs: &PaneInputs) -> PaneAnswerCore {
    match kimi_facts(inputs) {
        KimiResolved::Gap { .. } => {
            PaneAnswerCore { answer_state: AnswerState::BoundButUnacquired, band: Band::Dormant, within_band: None }
        }
        KimiResolved::Facts(facts) => PaneAnswerCore {
            answer_state: AnswerState::Answered,
            band: kimi_band(facts.available_balance),
            within_band: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_786_636_800_000; // 2026-08-12T16:00:00Z

    fn body_json(available: f64, voucher: f64, cash: f64) -> String {
        format!(
            r#"{{"available_balance":{available},"voucher_balance":{voucher},"cash_balance":{cash}}}"#
        )
    }

    fn ok_envelope(body: &str) -> serde_json::Value {
        serde_json::json!({"kind":"ok","schema":SOURCE,"body":body})
    }

    fn inputs_with(envelope: serde_json::Value) -> PaneInputs {
        serde_json::from_value(serde_json::json!({
            "nowMs": NOW,
            "paneReads": {
                SOURCE: {
                    "snapshots": [{
                        "key": SNAPSHOT_KEY,
                        "envelope": envelope,
                        "freshness": {"kind":"age","ageMs":60000,"declaredCadenceMs":21_600_000},
                    }],
                },
            },
        }))
        .unwrap()
    }

    fn inputs(available: f64, voucher: f64, cash: f64) -> PaneInputs {
        inputs_with(ok_envelope(&body_json(available, voucher, cash)))
    }

    fn empty_inputs() -> PaneInputs {
        serde_json::from_value(serde_json::json!({"nowMs": NOW})).unwrap()
    }

    fn gap_of(inputs: &PaneInputs) -> KimiGap {
        match kimi_facts(inputs) {
            KimiResolved::Gap { gap } => gap,
            KimiResolved::Facts(facts) => panic!("expected a gap, got {facts:?}"),
        }
    }

    fn facts_of(inputs: &PaneInputs) -> KimiFacts {
        match kimi_facts(inputs) {
            KimiResolved::Facts(facts) => facts,
            KimiResolved::Gap { gap } => panic!("expected facts, got {gap:?}"),
        }
    }

    // -------------------------------------------------------- parse_kimi_body

    #[test]
    fn reads_a_well_formed_body() {
        let inputs = inputs(12.4, 8.0, 4.4);
        assert_eq!(
            parse_kimi_body(inputs.snapshot(SOURCE, SNAPSHOT_KEY)),
            Ok(KimiBody { available_balance: 12.4, voucher_balance: 8.0, cash_balance: 4.4 }),
        );
    }

    #[test]
    fn reads_a_negative_cash_balance_without_rejecting_it() {
        let inputs = inputs(4.1, 5.1, -1.0);
        assert_eq!(
            parse_kimi_body(inputs.snapshot(SOURCE, SNAPSHOT_KEY)),
            Ok(KimiBody { available_balance: 4.1, voucher_balance: 5.1, cash_balance: -1.0 }),
        );
    }

    #[test]
    fn names_which_kind_of_gap_it_is_rather_than_answering_emptily() {
        let cases: Vec<(serde_json::Value, KimiGap)> = vec![
            (
                serde_json::json!({"kind":"malformed","reason":"`body` is missing"}),
                KimiGap::Malformed { reason: "`body` is missing".into() },
            ),
            (
                serde_json::json!({"kind":"ok","schema":"kimi-balance/v9","body":"{}"}),
                KimiGap::UnknownSchema { schema: "kimi-balance/v9".into() },
            ),
            (ok_envelope("{"), KimiGap::NotJson),
            (ok_envelope("[]"), KimiGap::NotAnObject),
            (
                ok_envelope(
                    &serde_json::json!({"available_balance":"not a number","voucher_balance":1,"cash_balance":1})
                        .to_string(),
                ),
                KimiGap::BadNumbers,
            ),
        ];
        for (envelope, expected) in cases {
            let inputs = inputs_with(envelope);
            assert_eq!(
                parse_kimi_body(inputs.snapshot(SOURCE, SNAPSHOT_KEY)),
                Err(expected.clone()),
                "{expected:?}",
            );
        }
    }

    #[test]
    fn an_absent_snapshot_is_not_fetched_rather_than_malformed() {
        let inputs = empty_inputs();
        assert_eq!(parse_kimi_body(inputs.snapshot(SOURCE, SNAPSHOT_KEY)), Err(KimiGap::NotFetched));
        assert_eq!(gap_of(&inputs), KimiGap::NotFetched);
    }

    // -------------------------------------------------------------- kimi_facts

    #[test]
    fn surfaces_the_negative_cash_split_on_the_resolved_facts() {
        let facts = facts_of(&inputs(4.1, 5.1, -1.0));
        assert_eq!(facts.cash_balance, -1.0);
    }

    #[test]
    fn never_reads_unknown_as_fresh_against_this_panes_own_threshold() {
        let mut inputs = inputs(12.4, 8.0, 4.4);
        let snapshot = inputs
            .pane_reads
            .get_mut(SOURCE)
            .and_then(|read| read.snapshots.first_mut())
            .unwrap();
        snapshot.freshness = FreshnessFact::Unknown;
        assert!(facts_of(&inputs).stale);
    }

    #[test]
    fn bands_stale_at_roughly_13h_against_the_6h_cadence_two_missed_polls() {
        let fresh = inputs_with(serde_json::json!({
            "kind": "ok", "schema": SOURCE, "body": body_json(12.4, 8.0, 4.4),
        }));
        let mut fresh: PaneInputs = fresh;
        fresh.pane_reads.get_mut(SOURCE).unwrap().snapshots[0].freshness =
            FreshnessFact::Age { age_ms: 12 * 60 * 60 * 1000, declared_cadence_ms: Some(21_600_000) };
        assert!(!facts_of(&fresh).stale);

        let mut stale = inputs(12.4, 8.0, 4.4);
        stale.pane_reads.get_mut(SOURCE).unwrap().snapshots[0].freshness =
            FreshnessFact::Age { age_ms: 14 * 60 * 60 * 1000, declared_cadence_ms: Some(21_600_000) };
        assert!(facts_of(&stale).stale);
    }

    // -------------------------------------------------------------- kimi_band

    #[test]
    fn bands_purely_off_available_balance_as_distance_from_the_zero_cliff() {
        assert_eq!(kimi_band(0.0), Band::Live);
        assert_eq!(kimi_band(-3.0), Band::Live);
        assert_eq!(kimi_band(0.5), Band::Imminent);
        assert_eq!(kimi_band(IMMINENT_THRESHOLD_USD), Band::Near);
        assert_eq!(kimi_band(3.0), Band::Near);
        assert_eq!(kimi_band(NEAR_THRESHOLD_USD), Band::Dormant);
        assert_eq!(kimi_band(50.0), Band::Dormant);
    }

    // -------------------------------------------------------------- kimi_answer

    #[test]
    fn answers_with_the_balance_banded_when_a_snapshot_has_landed() {
        let answer = kimi_answer(&inputs(12.4, 8.0, 4.4));
        assert_eq!(answer.answer_state, AnswerState::Answered);
        assert_eq!(answer.band, Band::Dormant);
        assert_eq!(answer.within_band, None);
    }

    #[test]
    fn is_a_gap_never_an_unbound_question_when_nothing_has_polled_yet() {
        let answer = kimi_answer(&empty_inputs());
        assert_eq!(answer.answer_state, AnswerState::BoundButUnacquired);
        assert_eq!(answer.band, Band::Dormant);
        assert_eq!(answer.within_band, None);
    }

    #[test]
    fn bands_live_once_the_balance_hits_the_cliff() {
        let answer = kimi_answer(&inputs(0.0, 8.0, 4.4));
        assert_eq!(answer.band, Band::Live);
    }
}
