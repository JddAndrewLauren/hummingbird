//! The standing-question panes' decision half (#533/M4, ADR-0025) — the
//! pane shell contract, the cross-pane sort, the zone bridge, and all nine
//! real panes.
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
//! # Scope, as it stands after #534
//!
//! #533's probe sank **waste alone**, shipping [`zone_queries`] and
//! [`rank_panes`] with a one-question list so the slice that followed would
//! grow [`SUNK`] rather than the surface API. **#534 did exactly that**:
//! it sank the remaining seven — the status four
//! (kimi/github/uptime/reachability) and the now three
//! (race/weekend/vacation) — leaving the surface pair unchanged, which is
//! the evidence that the probe's shape held.
//!
//! **The facts still cross per question, not as one union.** A
//! [`RankedPaneRecord`] carries identity and [`PaneAnswerCore`]; a
//! question's *facts* cross on their own per-question seam export
//! (`waste_facts_json`, and `waste::waste_facts` behind it), so each pane
//! adds a seam function rather than a row. #534 settled the question #533
//! left open and kept it that way: no caller ever needs a pane's full facts
//! and its rank in one call — the shell reads [`PaneAnswerCore`] to rank,
//! and a pane's facts only inside its own expanded render — so the union
//! would be a type nobody was blocked on (ADR-0025's #534 amendment). What
//! the acceptance criterion names is what holds: facts cross as structured
//! data and gaps cross as *kinds*, never as sentences.
//!
//! The web still ranks per question rather than hoisting to [`rank_panes`]
//! — that is a rendering-side move, not a decision, and no slice has asked
//! for it yet.
//!
//! Everything here is pure and clock-free: `now` arrives on
//! [`inputs::PaneInputs`], exactly as the module header in
//! [`super`](super) requires.

pub mod contract;
pub mod github;
pub mod homework;
pub mod inputs;
pub mod kimi;
pub mod race;
pub mod reachability;
pub mod scps;
pub mod sort;
pub mod uptime;
pub mod vacation;
pub mod waste;
pub mod weekend;
pub mod zone;

pub use contract::{
    pane_key, AnswerState, Band, PaneAnswerCore, RankedPaneRecord, StandingQuestion, Surface,
    BAND_ORDER, QUESTION_ORDER,
};
pub use inputs::PaneInputs;
pub use sort::{order_panes, same_pane_identity};
pub use zone::{ZoneFact, ZoneFacts, ZoneQuery};

/// Which questions this crate decides today, and which surface each
/// renders into. Grew from one row (waste alone, #533's probe) to all
/// eight at #534, which sank the remaining seven: the status four
/// (kimi/github/uptime/reachability) and the now three
/// (race/weekend/vacation). #675 added a ninth — [`homework`], the first
/// question **born** here rather than sunk from the web, and the first
/// keyed on the operator's own items rather than an outside source. The
/// web reads the same list to know which questions it may stop answering
/// itself. #693 added a tenth — [`scps`], the second question keyed on the
/// calendar arm with no per-question calendar binding (`weekend`'s own
/// shape, restated in `scps.rs`'s module header).
pub const SUNK: [(StandingQuestion, Surface); 10] = [
    (StandingQuestion::Homework, Surface::Now),
    (StandingQuestion::Scps, Surface::Now),
    (StandingQuestion::Waste, Surface::Now),
    (StandingQuestion::Weekend, Surface::Now),
    (StandingQuestion::Vacation, Surface::Now),
    (StandingQuestion::Race, Surface::Now),
    (StandingQuestion::Kimi, Surface::Status),
    (StandingQuestion::Github, Surface::Status),
    (StandingQuestion::Uptime, Surface::Status),
    (StandingQuestion::Reachability, Surface::Status),
];

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
            StandingQuestion::Homework => homework::homework_zone_queries(inputs),
            StandingQuestion::Scps => scps::scps_zone_queries(inputs),
            StandingQuestion::Waste => waste::waste_zone_queries(inputs),
            StandingQuestion::Weekend => weekend::weekend_zone_queries(inputs.now_ms),
            StandingQuestion::Vacation => vacation::vacation_zone_queries(inputs),
            // Race, kimi, github, uptime and reachability are all
            // instant-based (no civil-date reasoning), so they ask for
            // nothing on this half of the bridge.
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
        // unbound/never-polled: a pane that vanished when nobody had bound
        // it would be a question nobody could ever discover (ADR-0017's own
        // rule).
        let answered: Vec<(String, PaneAnswerCore)> = match question {
            StandingQuestion::Homework => {
                vec![(homework::SUBJECT_KEY.to_string(), homework::homework_answer(inputs, facts))]
            }
            StandingQuestion::Scps => {
                vec![(scps::SUBJECT_KEY.to_string(), scps::scps_answer(inputs, facts))]
            }
            StandingQuestion::Waste => {
                vec![(waste::SNAPSHOT_KEY.to_string(), waste::waste_answer(inputs, facts))]
            }
            StandingQuestion::Weekend => {
                vec![(weekend::SUBJECT_KEY.to_string(), weekend::weekend_answer(inputs, facts))]
            }
            StandingQuestion::Vacation => {
                vec![(vacation::SUBJECT_KEY.to_string(), vacation::vacation_answer(inputs, facts))]
            }
            StandingQuestion::Race => race::race_subjects(inputs)
                .into_iter()
                .map(|subject| {
                    let answer = race::race_answer(&subject, inputs);
                    (subject, answer)
                })
                .collect(),
            StandingQuestion::Kimi => {
                vec![(kimi::SNAPSHOT_KEY.to_string(), kimi::kimi_answer(inputs))]
            }
            StandingQuestion::Github => github::github_subjects(inputs)
                .into_iter()
                .map(|subject| {
                    let answer = github::github_answer(&subject, inputs);
                    (subject, answer)
                })
                .collect(),
            StandingQuestion::Uptime => uptime::uptime_subjects(inputs)
                .into_iter()
                .map(|subject| {
                    let answer = uptime::uptime_answer(&subject, inputs);
                    (subject, answer)
                })
                .collect(),
            StandingQuestion::Reachability => {
                vec![(reachability::SUBJECT_KEY.to_string(), reachability::reachability_answer(inputs))]
            }
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
        // Waste (2 queries) plus weekend's window (always asks, independent
        // of bindings) plus vacation's "today" query — more than waste
        // alone asked before #534, and never zero.
        assert!(now.len() > 2, "expected more than waste's own 2 queries, got {}", now.len());
        // Homework's own "what day is it here" query (#675) is one of them,
        // and it is deduplicated against the identical one weekend and
        // vacation ask — the whole point of keying by `ZoneQuery::key`.
        let today =
            ZoneQuery::CivilDate { zone: zone::DEVICE_ZONE.to_string(), at_ms: inputs.now_ms }
                .key();
        assert_eq!(now.iter().filter(|query| query.key() == today).count(), 1);
        // None of the Status four (kimi/github/uptime/reachability) is
        // civil-date reasoning, so Status still asks for nothing at all —
        // unchanged by #534 sinking them.
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
        // Waste (bound and answered) and homework (answered, and dormant on
        // an empty mirror — #675: nobody binds it), plus scps (answered,
        // dormant — no calendar connected here either, but scps has no
        // `unbound` arm at all, #693), plus weekend/vacation/race, each
        // unbound in this fixture (no calendar connected, no race-series
        // binding) — all still ranked, per ADR-0017's "a pane nobody has
        // bound yet must still be discoverable" rule.
        assert_eq!(ranked.len(), 6);
        let homework = ranked.iter().find(|pane| pane.question == "homework").unwrap();
        assert_eq!(homework.answer.answer_state, AnswerState::Answered);
        assert_eq!(homework.answer.band, Band::Dormant);
        let scps = ranked.iter().find(|pane| pane.question == "scps").unwrap();
        assert_eq!(scps.answer.answer_state, AnswerState::BoundButUnacquired);
        let waste = ranked.iter().find(|pane| pane.question == "waste").unwrap();
        assert_eq!(waste.pane_key, "waste:collection");
        assert_eq!(waste.answer.answer_state, AnswerState::Answered);
        assert_eq!(waste.answer.band, Band::Dormant);
        for question in ["weekend", "vacation", "race"] {
            let pane = ranked.iter().find(|pane| pane.question == question).unwrap();
            assert_eq!(pane.answer.answer_state, AnswerState::Unbound, "{question}");
        }
    }

    #[test]
    fn an_unbound_question_still_ranks_a_pane_rather_than_vanishing() {
        let mut inputs = bound_inputs();
        inputs.bindings = Some(Vec::new());
        let ranked = rank_panes(Surface::Now, &inputs, &ZoneFacts::default());
        // Every Now question is unanswerable in this fixture (waste's page
        // cleared, no calendar, no race series, and no resolved device
        // zone) — all six still rank.
        assert_eq!(ranked.len(), 6);
        // Homework and scps have no binding to be unset, so their
        // unanswerable state is the bridge's own gap rather than `unbound`:
        // there is no setup prompt to route anyone to (`homework.rs`'s
        // `homework_answer`; `scps.rs`'s own "never unbound" rule).
        for question in ["homework", "scps"] {
            let pane = ranked.iter().find(|pane| pane.question == question).unwrap();
            assert_eq!(pane.answer.answer_state, AnswerState::BoundButUnacquired, "{question}");
        }
        assert!(ranked
            .iter()
            .filter(|pane| pane.question != "homework" && pane.question != "scps")
            .all(|pane| pane.answer.answer_state == AnswerState::Unbound));
    }

    #[test]
    fn the_status_four_rank_from_defaults_with_no_binding_at_all() {
        // None of kimi/github/uptime/reachability has a per-device binding
        // (ADR-0017 decisions 2/5/6 and #316), so a completely empty
        // `PaneInputs` still ranks all four, each a never-polled sentinel.
        let inputs = PaneInputs::default();
        let ranked = rank_panes(Surface::Status, &inputs, &ZoneFacts::default());
        assert_eq!(ranked.len(), 4);
        assert!(ranked.iter().all(|pane| pane.answer.answer_state == AnswerState::BoundButUnacquired));
        let questions: Vec<&str> = ranked.iter().map(|pane| pane.question.as_str()).collect();
        assert_eq!(questions, vec!["kimi", "github", "uptime", "reachability"]);
    }
}
