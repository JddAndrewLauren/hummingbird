//! The standing-question panes' decision half (#533/M4, ADR-0025) — the
//! pane shell contract, the cross-pane sort, the zone bridge, and one real
//! pane (waste).
//!
//! # Why this family exists
//!
//! The panes were the last big body of web-only decision logic
//! (`client/web/src/screens/questions/` plus eight `*-pane/` directories),
//! and the obstacle unique to them is that panes are **civil-date
//! reasoning** while this crate owns no tzdb, deliberately and at a
//! measured price. [`zone`] is the answer: a two-phase crossing where the
//! core names every `(zone, civil-date)` fact it needs, the host resolves
//! them, and the core decides — including deciding what an unresolvable
//! zone means.
//!
//! # Scope, stated so the next slice does not have to guess
//!
//! This slice sinks **waste alone**. [`zone_queries`] and [`rank_panes`]
//! are the surface-level pair the whole family will be driven through, and
//! they ship now — with a one-question list — so #534 grows a list rather
//! than an API. The other seven questions are still answered entirely in
//! the web's own `*-pane/` modules and are simply not in [`SUNK`] yet;
//! a surface's ranked region is the union of both, which is why the web
//! keeps ranking per-question rather than hoisting to [`rank_panes`] in
//! this slice.
//!
//! Everything here is pure and clock-free: `now` arrives on
//! [`inputs::PaneInputs`], exactly as the module header in
//! [`super`](super) requires.

pub mod contract;
pub mod inputs;
pub mod sort;
pub mod waste;
pub mod zone;

pub use contract::{
    pane_key, AnswerState, Band, PaneAnswerCore, RankedPaneRecord, StandingQuestion, Surface,
    BAND_ORDER, QUESTION_ORDER,
};
pub use inputs::PaneInputs;
pub use sort::{order_panes, same_pane_identity};
pub use zone::{ZoneFact, ZoneFacts, ZoneQuery};

/// Which questions this crate decides today, and which surface each
/// renders into. Grows one row per pane as #534 runs; the web reads the
/// same list to know which questions it may stop answering itself.
pub const SUNK: [(StandingQuestion, Surface); 1] = [(StandingQuestion::Waste, Surface::Now)];

/// Every `(zone, civil-date)` fact `surface`'s sunk questions need, given
/// these inputs — the first half of the two-phase crossing.
///
/// Deduplicated by [`ZoneQuery::key`]: two questions asking the same thing
/// cost the host one lookup, and the order is the declaration order of
/// [`SUNK`] so a host may cache against a stable list.
pub fn zone_queries(surface: Surface, inputs: &PaneInputs) -> Vec<ZoneQuery> {
    let mut queries: Vec<ZoneQuery> = Vec::new();
    for (question, declared) in SUNK {
        if declared != surface {
            continue;
        }
        let asked = match question {
            StandingQuestion::Waste => waste::waste_zone_queries(inputs),
            _ => Vec::new(),
        };
        for query in asked {
            if !queries.iter().any(|existing| existing.key() == query.key()) {
                queries.push(query);
            }
        }
    }
    queries
}

/// `surface`'s sunk questions, ranked — the second half of the crossing.
///
/// Returns the panes **in display order** ([`order_panes`] against
/// [`QUESTION_ORDER`]), so a caller that has no other questions to union in
/// can render the result directly.
pub fn rank_panes(
    surface: Surface,
    inputs: &PaneInputs,
    facts: &ZoneFacts,
) -> Vec<RankedPaneRecord> {
    let mut panes: Vec<RankedPaneRecord> = Vec::new();
    for (question, declared) in SUNK {
        if declared != surface {
            continue;
        }
        // Every question returns its subjects, including a sentinel while
        // unbound: a pane that vanished when nobody had bound it would be a
        // question nobody could ever discover (ADR-0017's own rule).
        let answered = match question {
            StandingQuestion::Waste => {
                vec![(waste::SNAPSHOT_KEY.to_string(), waste::waste_answer(inputs, facts))]
            }
            _ => Vec::new(),
        };
        for (subject_key, answer) in answered {
            panes.push(RankedPaneRecord {
                pane_key: pane_key(question.as_str(), &subject_key),
                question: question.as_str().to_string(),
                subject_key,
                answer,
            });
        }
    }
    let order: Vec<String> =
        QUESTION_ORDER.iter().map(|question| question.as_str().to_string()).collect();
    order_panes(&panes, &order)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bound_inputs() -> PaneInputs {
        serde_json::from_value(serde_json::json!({
            "nowMs": 1_786_377_600_000i64,
            "bindings": [
                {"key": waste::BINDING_KEY, "value": {"state":"text","text":"https://example.gov"}}
            ],
            "paneReads": {
                waste::SOURCE: {"snapshots": [{
                    "key": waste::SNAPSHOT_KEY,
                    "envelope": {"kind":"ok","schema":waste::SOURCE,
                                 "body":"{\"zone\":\"America/Los_Angeles\",\"scheduled\":\"2026-08-17\",\"collected_on\":\"2026-08-17\",\"streams\":[\"trash\"]}"},
                    "freshness": {"kind":"age","ageMs":60000,"declaredCadenceMs":86400000},
                }]},
            },
        }))
        .unwrap()
    }

    #[test]
    fn a_surface_asks_for_its_own_questions_facts_and_nobody_elses() {
        let inputs = bound_inputs();
        let now = zone_queries(Surface::Now, &inputs);
        assert_eq!(now.len(), 2);
        // Nothing is sunk onto Status yet, and a surface with no sunk
        // questions asks for nothing rather than for everything.
        assert!(zone_queries(Surface::Status, &inputs).is_empty());
    }

    #[test]
    fn the_query_list_is_deduplicated_by_key() {
        let inputs = bound_inputs();
        let queries = zone_queries(Surface::Now, &inputs);
        let mut keys: Vec<String> = queries.iter().map(|query| query.key()).collect();
        let before = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), before);
    }

    #[test]
    fn ranking_a_surface_yields_one_record_per_subject_with_a_stable_identity() {
        let inputs = bound_inputs();
        let facts = {
            let mut facts = ZoneFacts::default();
            for query in zone_queries(Surface::Now, &inputs) {
                match &query {
                    ZoneQuery::CivilDate { .. } => {
                        facts.insert(&query, ZoneFact::Date("2026-08-10".into()))
                    }
                    ZoneQuery::Midnight { .. } => {
                        facts.insert(&query, ZoneFact::Instant(1_786_950_000_000))
                    }
                }
            }
            facts
        };
        let ranked = rank_panes(Surface::Now, &inputs, &facts);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].pane_key, "waste:collection");
        assert_eq!(ranked[0].question, "waste");
        assert_eq!(ranked[0].answer.answer_state, AnswerState::Answered);
        assert_eq!(ranked[0].answer.band, Band::Dormant);
    }

    #[test]
    fn an_unbound_question_still_ranks_a_pane_rather_than_vanishing() {
        let mut inputs = bound_inputs();
        inputs.bindings = Some(Vec::new());
        let ranked = rank_panes(Surface::Now, &inputs, &ZoneFacts::default());
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].answer.answer_state, AnswerState::Unbound);
    }
}
