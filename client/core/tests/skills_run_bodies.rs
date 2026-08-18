//! The Rust side of the shared run-body fixture (#538).
//!
//! `tests/fixtures/skills-run-bodies.json` is read by three languages —
//! this file, `client/web/src/skills/run-body-fixture.test.ts`, and the
//! Android instrumented suite. Each asserts its own builder produces
//! `expected` byte for byte, so the three cannot drift while each stays
//! green against a fixture of its own. The precedent for reading another
//! crate's committed fixture across a language boundary is
//! `client/web/src/screens/race-pane/race.test.ts` reading
//! `server/race-poll/tests/fixtures/golden-body.json`.
//!
//! This is an integration test rather than an inline `mod tests` because
//! the fixture is a file on disk, and `tests/` is where this crate already
//! keeps its file-backed pins (`task_mirror_golden.rs`,
//! `google_adapter_fixtures.rs`).

use hummingbird_core::decisions::skills::{
    args::{grill_run_body, microtask_run_body, GrillTurn, MicrotaskRunInput},
    envelope::GrillQuestion,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    skill: String,
    input: serde_json::Value,
    expected: String,
}

/// The grill input's shape, which — unlike the microtask one — is not a
/// struct in the core: `grill_run_body` takes its two arguments directly,
/// because nothing but a test ever has them in one bag.
#[derive(Deserialize)]
struct GrillInput {
    #[serde(rename = "ref")]
    reference: String,
    turns: Vec<GrillTurn>,
}

fn fixture() -> Fixture {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/skills-run-bodies.json"
    ))
    .expect("the shared run-body fixture must be readable");
    serde_json::from_str(&text).expect("the shared run-body fixture must parse")
}

#[test]
fn every_fixture_case_matches_the_builders_bytes() {
    let fixture = fixture();
    assert!(!fixture.cases.is_empty(), "the fixture must carry cases");
    for case in &fixture.cases {
        let built = match case.skill.as_str() {
            "microtask" => {
                let input: MicrotaskRunInput = serde_json::from_value(case.input.clone())
                    .unwrap_or_else(|error| panic!("{}: bad microtask input — {error}", case.name));
                microtask_run_body(&input)
            }
            "grill-me" => {
                let input: GrillInput = serde_json::from_value(case.input.clone())
                    .unwrap_or_else(|error| panic!("{}: bad grill input — {error}", case.name));
                grill_run_body(&input.reference, &input.turns)
            }
            other => panic!("{}: unknown skill {other}", case.name),
        };
        assert_eq!(built, case.expected, "{}", case.name);
    }
}

/// The fixture is only a *cross-language* pin if it covers both skills and
/// both edges of the omit-when-unset rule. A fixture that quietly lost its
/// grill cases would still pass the assertion above.
#[test]
fn the_fixture_covers_both_skills_and_the_omitted_args() {
    let fixture = fixture();
    let skills: Vec<&str> = fixture.cases.iter().map(|case| case.skill.as_str()).collect();
    assert!(skills.contains(&"microtask"), "no microtask case");
    assert!(skills.contains(&"grill-me"), "no grill-me case");
    assert!(
        fixture.cases.iter().any(|case| case.expected == r#"{"skill":"microtask","args":{"ref":"i"}}"#),
        "no case pins that an unset arg is omitted rather than sent",
    );
    assert!(
        fixture.cases.iter().any(|case| case.expected.contains("grain")),
        "no case pins a set optional arg",
    );
}

/// The runner accepts free text in an answer, and a raw newline or quote in
/// one is a body this client must still be able to post. Pinned here rather
/// than left to the case list alone, because an escaping bug is exactly the
/// class of defect a hand-typed per-language fixture would hide.
#[test]
fn an_answer_needing_json_escapes_survives_the_builder() {
    let turns = vec![GrillTurn {
        question: GrillQuestion {
            prompt: "Say it".to_string(),
            recommended_answer: "anything".to_string(),
            choices: vec!["a".to_string(), "b".to_string()],
        },
        answer: "he said \"soon\"\nand left".to_string(),
    }];
    let body = grill_run_body("i", &turns);
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("the body must be valid JSON");
    assert_eq!(parsed["args"]["turns"][0]["answer"], serde_json::json!("he said \"soon\"\nand left"));
}
