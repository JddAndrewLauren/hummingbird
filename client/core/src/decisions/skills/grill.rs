//! The state of one Grill turn request (#355, ADR-0023; sunk here from
//! `client/web/src/skills/grill-turn-state.ts` at #538) — [`super::run`]'s
//! [`super::run::reduce_run`] for `microtask`, adapted for `grill-me`'s own
//! terminal shape: an `ok` line answers with either the next `question` or
//! the terminal `proposal` ([`super::envelope::grill_result`]), never a bare
//! `note`, so the "done" phase there has no equivalent here — this reducer
//! grows a `Question` and a `Proposal` phase in its place.
//!
//! Same invariants [`super::run`] documents for its own reducer: "in
//! flight", "declined" and each of the two answered shapes are
//! distinguishable phases, and the duplicate-tap rule (a `Started` while
//! already `Asking` is a no-op) lives here rather than in a click handler.

use serde::{Deserialize, Serialize};

use super::envelope::{grill_result, GrillProposal, GrillQuestion, GrillTurnResult};
use super::run::SkillEvent;

/// A run that answered `ok:true` with a result outside `grill-me`'s own
/// `oneOf` schema. The runner's schema validation is meant to make this
/// unreachable in practice (`.claude/skills/grill-me/SKILL.md`: "Your only
/// failure mode is answering outside the schema") — this is the client's
/// own backstop for that promise not holding, not a case this reducer
/// expects to exercise against a well-behaved runner.
pub const OUTSIDE_SCHEMA: &str = "The run answered outside the schema.";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "lowercase")]
pub enum GrillTurnState {
    Idle,
    Asking {
        messages: Vec<String>,
    },
    Question {
        messages: Vec<String>,
        question: GrillQuestion,
        backend: Option<String>,
        model: Option<String>,
    },
    Proposal {
        messages: Vec<String>,
        proposal: GrillProposal,
        backend: Option<String>,
        model: Option<String>,
    },
    Declined {
        messages: Vec<String>,
        /// Verbatim: the seam's own words, or the transport's. Never
        /// prefixed, never branched on — same rule [`super::run`] states
        /// for its own `reason`.
        reason: String,
        backend: Option<String>,
        model: Option<String>,
        answered: bool,
    },
}

pub const IDLE: GrillTurnState = GrillTurnState::Idle;

pub fn reduce_grill_turn(state: &GrillTurnState, event: &SkillEvent) -> GrillTurnState {
    match event {
        SkillEvent::Started => match state {
            GrillTurnState::Asking { .. } => state.clone(),
            _ => GrillTurnState::Asking { messages: Vec::new() },
        },

        SkillEvent::Progress { message } => match state {
            GrillTurnState::Asking { messages } => {
                GrillTurnState::Asking { messages: append(messages, message) }
            }
            _ => state.clone(),
        },

        SkillEvent::Ok { result, backend, model } => {
            let GrillTurnState::Asking { messages } = state else {
                return state.clone();
            };
            match grill_result(result) {
                // Answered — by a backend, with something — so `answered`
                // is true even though the answer was unusable.
                None => declined(messages, OUTSIDE_SCHEMA, backend, model, true),
                Some(GrillTurnResult::Question { question }) => GrillTurnState::Question {
                    messages: messages.clone(),
                    question,
                    backend: backend.clone(),
                    model: model.clone(),
                },
                Some(GrillTurnResult::Proposal { proposal }) => GrillTurnState::Proposal {
                    messages: messages.clone(),
                    proposal,
                    backend: backend.clone(),
                    model: model.clone(),
                },
            }
        }

        SkillEvent::Failed { error, backend, model, answered } => match state {
            GrillTurnState::Asking { messages } => {
                declined(messages, error, backend, model, *answered)
            }
            _ => state.clone(),
        },

        SkillEvent::Unreadable => state.clone(),
    }
}

fn declined(
    messages: &[String],
    reason: &str,
    backend: &Option<String>,
    model: &Option<String>,
    answered: bool,
) -> GrillTurnState {
    GrillTurnState::Declined {
        messages: messages.to_vec(),
        reason: reason.to_string(),
        backend: backend.clone(),
        model: model.clone(),
        answered,
    }
}

/// The runner heartbeats `"still running"` every 20s — same de-duplication
/// [`super::run`]'s own `append` applies, and the same reason it is not a
/// timer.
fn append(messages: &[String], message: &str) -> Vec<String> {
    if messages.last().map(String::as_str) == Some(message) {
        return messages.to_vec();
    }
    let mut grown = messages.to_vec();
    grown.push(message.to_string());
    grown
}

#[cfg(test)]
mod tests {
    use super::*;
    use hummingbird_domain::GrillVerdict;
    use serde_json::{json, Map, Value};

    /// The cases ported from `grill-turn-state.test.ts`.
    fn drive(events: Vec<SkillEvent>) -> GrillTurnState {
        events.iter().fold(IDLE, |state, event| reduce_grill_turn(&state, event))
    }

    fn progress(message: &str) -> SkillEvent {
        SkillEvent::Progress { message: message.to_string() }
    }

    fn question() -> GrillQuestion {
        GrillQuestion {
            prompt: "Which airport?".to_string(),
            recommended_answer: "SEA".to_string(),
            choices: vec!["SEA".to_string(), "PDX".to_string()],
        }
    }

    fn proposal() -> GrillProposal {
        let mut patch = Map::new();
        patch.insert("title".to_string(), json!("book flights"));
        GrillProposal {
            summary: "Settled on SEA".to_string(),
            verdict: GrillVerdict::Resolved,
            patch,
        }
    }

    fn ok_question() -> SkillEvent {
        SkillEvent::Ok {
            result: json!({"kind": "question", "question": question()}),
            backend: Some("cloud".to_string()),
            model: Some("m".to_string()),
        }
    }

    fn ok_proposal() -> SkillEvent {
        SkillEvent::Ok {
            result: json!({"kind": "proposal", "proposal": proposal()}),
            backend: Some("cloud".to_string()),
            model: Some("m".to_string()),
        }
    }

    #[test]
    fn a_tap_starts_an_empty_asking_state() {
        assert_eq!(
            reduce_grill_turn(&IDLE, &SkillEvent::Started),
            GrillTurnState::Asking { messages: Vec::new() },
        );
    }

    #[test]
    fn a_second_start_while_asking_leaves_the_state_untouched() {
        let asking = drive(vec![SkillEvent::Started, progress("one")]);
        assert_eq!(reduce_grill_turn(&asking, &SkillEvent::Started), asking);
    }

    #[test]
    fn progress_accumulates_in_order_collapsing_consecutive_duplicates() {
        let state = drive(vec![
            SkillEvent::Started,
            progress("reading HB-42"),
            progress("reading HB-42"),
            progress("asking"),
        ]);
        assert_eq!(
            state,
            GrillTurnState::Asking {
                messages: vec!["reading HB-42".to_string(), "asking".to_string()],
            },
        );
    }

    #[test]
    fn an_ok_terminal_carrying_a_question_moves_to_the_question_phase_keeping_the_narration() {
        let state = drive(vec![SkillEvent::Started, progress("reading"), ok_question()]);
        assert_eq!(
            state,
            GrillTurnState::Question {
                messages: vec!["reading".to_string()],
                question: question(),
                backend: Some("cloud".to_string()),
                model: Some("m".to_string()),
            },
        );
    }

    #[test]
    fn an_ok_terminal_carrying_a_proposal_moves_to_the_proposal_phase() {
        let state = drive(vec![SkillEvent::Started, ok_proposal()]);
        assert_eq!(
            state,
            GrillTurnState::Proposal {
                messages: Vec::new(),
                proposal: proposal(),
                backend: Some("cloud".to_string()),
                model: Some("m".to_string()),
            },
        );
    }

    #[test]
    fn an_ok_terminal_outside_the_schema_declines_with_a_named_reason_evidencing_an_answer() {
        let state = drive(vec![
            SkillEvent::Started,
            SkillEvent::Ok {
                result: json!({"kind": "neither"}),
                backend: Some("cloud".to_string()),
                model: Some("m".to_string()),
            },
        ]);
        assert_eq!(
            state,
            GrillTurnState::Declined {
                messages: Vec::new(),
                reason: OUTSIDE_SCHEMA.to_string(),
                backend: Some("cloud".to_string()),
                model: Some("m".to_string()),
                answered: true,
            },
        );
    }

    #[test]
    fn a_failed_terminal_declines_verbatim_unprefixed() {
        let reason = "No device token on this device. Enter one in Settings, then try again.";
        let state = drive(vec![
            SkillEvent::Started,
            SkillEvent::Failed {
                error: reason.to_string(),
                backend: None,
                model: None,
                answered: false,
            },
        ]);
        assert_eq!(
            state,
            GrillTurnState::Declined {
                messages: Vec::new(),
                reason: reason.to_string(),
                backend: None,
                model: None,
                answered: false,
            },
        );
    }

    #[test]
    fn a_routed_failures_answered_flag_carries_through() {
        let state = drive(vec![
            SkillEvent::Started,
            SkillEvent::Failed {
                error: "That item already has live steps.".to_string(),
                backend: Some("cloud".to_string()),
                model: Some("m".to_string()),
                answered: true,
            },
        ]);
        assert!(matches!(state, GrillTurnState::Declined { answered: true, .. }));
    }

    /// Same rule [`super::run`] states for `reduce_run`: a line arriving
    /// before a tap, or after the terminal one, must not reopen a finished
    /// turn.
    #[test]
    fn progress_ok_and_failed_are_all_no_ops_from_a_settled_state() {
        for settled in [
            IDLE,
            drive(vec![SkillEvent::Started, ok_question()]),
            drive(vec![SkillEvent::Started, ok_proposal()]),
            drive(vec![
                SkillEvent::Started,
                SkillEvent::Failed {
                    error: "nope".to_string(),
                    backend: None,
                    model: None,
                    answered: false,
                },
            ]),
        ] {
            assert_eq!(reduce_grill_turn(&settled, &progress("late")), settled);
            assert_eq!(reduce_grill_turn(&settled, &ok_question()), settled);
            assert_eq!(
                reduce_grill_turn(
                    &settled,
                    &SkillEvent::Failed {
                        error: "late".to_string(),
                        backend: None,
                        model: None,
                        answered: false,
                    },
                ),
                settled,
            );
        }
    }

    #[test]
    fn an_unreadable_line_is_dropped_never_terminal() {
        let state =
            drive(vec![SkillEvent::Started, SkillEvent::Unreadable, progress("still here")]);
        assert_eq!(
            state,
            GrillTurnState::Asking { messages: vec!["still here".to_string()] },
        );
    }

    /// Same round-trip the run reducer's own suite pins, for the same
    /// reason: the web wrapper compares serialized forms.
    #[test]
    fn every_phase_round_trips_through_its_json() {
        for state in [
            IDLE,
            drive(vec![SkillEvent::Started, progress("a")]),
            drive(vec![SkillEvent::Started, ok_question()]),
            drive(vec![SkillEvent::Started, ok_proposal()]),
            drive(vec![
                SkillEvent::Started,
                SkillEvent::Failed {
                    error: "no".to_string(),
                    backend: None,
                    model: None,
                    answered: true,
                },
            ]),
        ] {
            let text = serde_json::to_string(&state).unwrap();
            let back: GrillTurnState = serde_json::from_str(&text).unwrap();
            assert_eq!(back, state, "{text}");
        }
    }

    /// The proposal's `patch` is opaque — whatever the interview turned up
    /// survives the round trip untouched, keys this family has never heard
    /// of included.
    #[test]
    fn an_unknown_patch_key_survives_the_round_trip_unread() {
        let mut patch = Map::new();
        patch.insert("a_field_this_client_never_heard_of".to_string(), json!({"nested": [1, 2]}));
        let event = SkillEvent::Ok {
            result: json!({
                "kind": "proposal",
                "proposal": {"summary": "s", "verdict": "fog_remains", "patch": patch},
            }),
            backend: None,
            model: None,
        };
        let state = drive(vec![SkillEvent::Started, event]);
        let GrillTurnState::Proposal { proposal, .. } = &state else {
            panic!("expected a proposal phase");
        };
        assert_eq!(proposal.patch.get("a_field_this_client_never_heard_of"), Some(&json!({"nested": [1, 2]})));
        let text = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<GrillTurnState>(&text).unwrap(), state);
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["proposal"]["verdict"], json!("fog_remains"));
    }
}
