//! The Rust half of the panes' **shared** fixtures (#533/M4).
//!
//! `tests/fixtures/panes/*.json` is read by this file and, byte for byte,
//! by `client/web/src/screens/questions/shared-fixtures.test.ts`. One
//! artifact, two clients, no second copy to drift — the precedent is the
//! race pane's vitest reading `server/race-poll/tests/fixtures/golden-body
//! .json` off disk (#119).
//!
//! **Why a tzdb-free Rust suite can run these at all.** Each waste scenario
//! carries a `zoneFacts` table: the resolved half of `zone.rs`'s bridge,
//! keyed by `ZoneQuery::key()`. This side takes the table as given and
//! decides; the TS side additionally asserts its own `Intl` resolver
//! *reproduces* the table before running a scenario. That assertion is the
//! actual cross-host pin, and it is what a `java.time` resolver will be
//! held to at #536.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use hummingbird_core::decisions::panes::contract::{
    pane_key, AnswerState, Band, PaneAnswerCore, RankedPaneRecord,
};
use hummingbird_core::decisions::panes::inputs::PaneInputs;
use hummingbird_core::decisions::panes::sort::{order_panes, same_pane_identity};
use hummingbird_core::decisions::panes::waste::{
    waste_answer, waste_facts, WasteResolved, SNAPSHOT_KEY, SOURCE,
};
use hummingbird_core::decisions::panes::zone::ZoneFacts;
use serde::Deserialize;
use serde_json::{json, Value};

fn fixture(name: &str) -> Value {
    let path: PathBuf =
        [env!("CARGO_MANIFEST_DIR"), "tests", "fixtures", "panes", name].iter().collect();
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path:?}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parsing {path:?}: {e}"))
}

// ------------------------------------------------------------------- waste

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WasteScenario {
    name: String,
    now_ms: i64,
    bindings: Value,
    snapshot: Value,
    zone_facts: ZoneFacts,
    expected: WasteExpected,
}

/// Structured only — never a sentence. Gap wording is per-client by
/// ADR-0025, so a fixture asserting words would pin a rendering.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WasteExpected {
    answer_state: AnswerState,
    band: Band,
    within_band: Option<i64>,
    days_away: Option<i64>,
    holiday: Option<bool>,
    weekday_index: Option<u8>,
    stale: Option<bool>,
    gap_kind: Option<String>,
}

/// Rebuilds the host's `QuestionInputs`-shaped payload from the scenario —
/// deliberately including the `null` snapshot case as an *absent source*
/// rather than a source with no rows, since the fixture's "nothing has been
/// fetched yet" means the former.
fn waste_inputs(scenario: &WasteScenario) -> PaneInputs {
    let snapshots = if scenario.snapshot.is_null() {
        json!([])
    } else {
        json!([scenario.snapshot])
    };
    serde_json::from_value(json!({
        "nowMs": scenario.now_ms,
        "bindings": scenario.bindings,
        "paneReads": { SOURCE: { "source": SOURCE, "snapshots": snapshots, "liveAlerts": [] } },
    }))
    .expect("the fixture's inputs are PaneInputs-shaped")
}

#[test]
fn every_waste_scenario_answers_the_way_both_clients_must() {
    let fixture = fixture("waste-scenarios.json");
    let scenarios: Vec<WasteScenario> =
        serde_json::from_value(fixture["scenarios"].clone()).expect("waste scenarios");
    assert!(scenarios.len() >= 20, "the fixture lost scenarios: {}", scenarios.len());

    for scenario in &scenarios {
        let inputs = waste_inputs(scenario);
        let name = &scenario.name;

        let answer = waste_answer(&inputs, &scenario.zone_facts);
        assert_eq!(answer.answer_state, scenario.expected.answer_state, "answerState — {name}");
        assert_eq!(answer.band, scenario.expected.band, "band — {name}");
        assert_eq!(answer.within_band, scenario.expected.within_band, "withinBand — {name}");

        match waste_facts(&inputs, &scenario.zone_facts) {
            WasteResolved::Facts(facts) => {
                assert_eq!(scenario.expected.gap_kind, None, "expected a gap — {name}");
                assert_eq!(Some(facts.days_away), scenario.expected.days_away, "daysAway — {name}");
                assert_eq!(Some(facts.holiday), scenario.expected.holiday, "holiday — {name}");
                assert_eq!(
                    Some(facts.weekday_index),
                    scenario.expected.weekday_index,
                    "weekdayIndex — {name}",
                );
                assert_eq!(Some(facts.stale), scenario.expected.stale, "stale — {name}");
                // Never negative: a past collection is a gap, not a small
                // number.
                assert!(facts.days_away >= 0, "a past collection was rendered — {name}");
            }
            WasteResolved::Gap { gap } => {
                let tag = serde_json::to_value(&gap).unwrap()["gap"].as_str().unwrap().to_string();
                assert_eq!(
                    Some(tag),
                    scenario.expected.gap_kind,
                    "gapKind — {name}",
                );
            }
        }
    }
}

#[test]
fn no_waste_scenario_carries_a_rendered_sentence_across_the_seam() {
    // The manual read-through in the plan's verification list, as a test:
    // the only free text `WasteFacts`/`WasteGap` may carry is the domain's
    // own `EnvelopeProblem` wording on the `malformed` arm.
    let fixture = fixture("waste-scenarios.json");
    let scenarios: Vec<WasteScenario> =
        serde_json::from_value(fixture["scenarios"].clone()).expect("waste scenarios");
    for scenario in &scenarios {
        let inputs = waste_inputs(scenario);
        if let WasteResolved::Facts(facts) = waste_facts(&inputs, &scenario.zone_facts) {
            let json = serde_json::to_value(&facts).unwrap();
            for field in ["today", "collectedOn", "scheduled"] {
                assert!(
                    !json[field].as_str().unwrap().contains(' '),
                    "{field} looks like a sentence — {}",
                    scenario.name,
                );
            }
        }
    }
}

// -------------------------------------------------------------------- sort

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaneRow {
    question: String,
    subject_key: String,
    answer_state: AnswerState,
    band: Band,
    within_band: Option<i64>,
}

impl PaneRow {
    fn to_record(&self) -> RankedPaneRecord {
        RankedPaneRecord {
            question: self.question.clone(),
            subject_key: self.subject_key.clone(),
            pane_key: pane_key(&self.question, &self.subject_key),
            answer: PaneAnswerCore {
                answer_state: self.answer_state,
                band: self.band,
                within_band: self.within_band,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderScenario {
    name: String,
    question_order: Vec<String>,
    panes: Vec<PaneRow>,
    expected: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct IdentityScenario {
    name: String,
    a: Vec<PaneRow>,
    b: Vec<PaneRow>,
    expected: bool,
}

#[test]
fn every_sort_scenario_orders_the_way_both_clients_must() {
    let fixture = fixture("sort-scenarios.json");
    let scenarios: Vec<OrderScenario> =
        serde_json::from_value(fixture["order"].clone()).expect("order scenarios");
    assert!(scenarios.len() >= 7, "the fixture lost scenarios: {}", scenarios.len());

    for scenario in &scenarios {
        let panes: Vec<RankedPaneRecord> =
            scenario.panes.iter().map(PaneRow::to_record).collect();
        let ordered: Vec<String> = order_panes(&panes, &scenario.question_order)
            .into_iter()
            .map(|pane| pane.pane_key)
            .collect();
        assert_eq!(ordered, scenario.expected, "{}", scenario.name);
    }
}

#[test]
fn every_identity_scenario_answers_the_way_both_clients_must() {
    let fixture = fixture("sort-scenarios.json");
    let scenarios: Vec<IdentityScenario> =
        serde_json::from_value(fixture["identity"].clone()).expect("identity scenarios");
    assert!(scenarios.len() >= 5, "the fixture lost scenarios: {}", scenarios.len());

    for scenario in &scenarios {
        let a: Vec<RankedPaneRecord> = scenario.a.iter().map(PaneRow::to_record).collect();
        let b: Vec<RankedPaneRecord> = scenario.b.iter().map(PaneRow::to_record).collect();
        assert_eq!(same_pane_identity(&a, &b), scenario.expected, "{}", scenario.name);
    }
}

/// The keys the fixture hands the core have to be the keys the core asks
/// for, or every "resolved" scenario is silently running on an empty table
/// and asserting the unresolvable-zone arm by accident.
#[test]
fn each_resolved_scenarios_fact_table_answers_the_queries_the_core_actually_asks() {
    let fixture = fixture("waste-scenarios.json");
    let scenarios: Vec<WasteScenario> =
        serde_json::from_value(fixture["scenarios"].clone()).expect("waste scenarios");
    let raw: Vec<HashMap<String, Value>> = serde_json::from_value(
        fixture["scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["zoneFacts"].clone())
            .collect::<Value>(),
    )
    .expect("zoneFacts tables");

    for (scenario, table) in scenarios.iter().zip(raw.iter()) {
        let inputs = waste_inputs(scenario);
        let asked: Vec<String> =
            hummingbird_core::decisions::panes::waste::waste_zone_queries(&inputs)
                .iter()
                .map(|query| query.key())
                .collect();
        for key in table.keys() {
            assert!(
                asked.contains(key),
                "{}: the fixture answers `{key}`, which the core never asks for",
                scenario.name,
            );
        }
        // A scenario the core asks nothing for cannot carry facts, and one
        // it asks about must either be fully answered or fully unanswered —
        // a half-resolved zone is not a state any host produces.
        assert!(
            table.is_empty() || table.len() == asked.len(),
            "{}: {} facts for {} queries",
            scenario.name,
            table.len(),
            asked.len(),
        );
    }
    // The `collection` key is the subject every scenario is about; a typo
    // here would make every snapshot invisible and every scenario a gap.
    assert_eq!(SNAPSHOT_KEY, "collection");
}
