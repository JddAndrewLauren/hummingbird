//! The state of one skill run, as something a reader can observe (#273,
//! sunk here from `client/web/src/skills/run-state.ts` at #538).
//!
//! The acceptance criterion is that "in flight", "declined with reason" and
//! "completed" are distinguishable — so they are four named phases rather
//! than a bag of booleans, and every phase has exactly one rendering.
//!
//! **The duplicate-tap rule lives here**, in [`reduce_run`]: a
//! [`SkillEvent::Started`] while a run is already streaming leaves the
//! state untouched. Putting it in the reducer rather than in a click
//! handler is what makes it testable without a DOM on any platform, and it
//! is why a button's `disabled` is a second expression of the rule rather
//! than the only one.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::envelope::{microtask_result, SkillLine};

/// What the reducer consumes: a tap, or a line, or a line **routing** has
/// annotated with the one fact no line carries — whether a backend actually
/// answered this run (#274).
///
/// `answered` is not a property of a line: the wire never sends it and
/// [`super::envelope::classify_line`] never sets it. Only the transport
/// knows it, from whether its request resolved, and the distinction is not
/// recoverable downstream — a seam decline (#307), a 401 and a rejected
/// connection all arrive as an unstamped `failed`, and
/// [`super::decline`] forbids telling them apart by their prose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SkillEvent {
    Started,
    Progress {
        message: String,
    },
    Ok {
        #[serde(default)]
        result: Value,
        backend: Option<String>,
        model: Option<String>,
    },
    Failed {
        error: String,
        backend: Option<String>,
        model: Option<String>,
        /// Absent means unrouted, which is the safe reading: "nothing told
        /// me a backend answered", never "a backend definitely did not".
        #[serde(default)]
        answered: bool,
    },
    Unreadable,
}

impl From<SkillLine> for SkillEvent {
    fn from(line: SkillLine) -> SkillEvent {
        match line {
            SkillLine::Progress { message } => SkillEvent::Progress { message },
            SkillLine::Ok { result, backend, model } => SkillEvent::Ok { result, backend, model },
            SkillLine::Failed { error, backend, model } => {
                SkillEvent::Failed { error, backend, model, answered: false }
            }
            SkillLine::Unreadable => SkillEvent::Unreadable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "lowercase")]
pub enum SkillRunState {
    Idle,
    Running {
        messages: Vec<String>,
    },
    Done {
        messages: Vec<String>,
        /// `microtask`'s own `note` — #307 point 7 puts what-was-kept there
        /// on purpose. Empty when the result did not carry one.
        note: String,
        backend: Option<String>,
        model: Option<String>,
    },
    Declined {
        messages: Vec<String>,
        /// Verbatim: the seam's own words, or the transport's. Never
        /// prefixed, never branched on.
        reason: String,
        backend: Option<String>,
        model: Option<String>,
        /// Whether a backend answered this run (#274). A decline that a
        /// backend *answered* is not evidence any backend is unreachable,
        /// so nothing may offer switching away from one on the strength of
        /// it.
        answered: bool,
    },
}

pub const IDLE: SkillRunState = SkillRunState::Idle;

pub fn is_running(state: &SkillRunState) -> bool {
    matches!(state, SkillRunState::Running { .. })
}

/// The stamp as one line of text, or `None` when there is nothing honest to
/// say. Absent when the envelope named no backend — an unstamped line means
/// nothing was attempted (ADR-0018), and a stamp invented at the render
/// site is exactly what #273 forbids.
pub fn stamp_label(state: &SkillRunState) -> Option<String> {
    let (backend, model) = match state {
        SkillRunState::Done { backend, model, .. } => (backend, model),
        SkillRunState::Declined { backend, model, .. } => (backend, model),
        _ => return None,
    };
    let backend = backend.as_deref()?;
    Some(match model.as_deref() {
        Some(model) => format!("{backend} · {model}"),
        None => backend.to_string(),
    })
}

pub fn reduce_run(state: &SkillRunState, event: &SkillEvent) -> SkillRunState {
    match event {
        // The duplicate-tap rule. A second tap during a streaming run must
        // not wipe the narration already on screen, and must not start
        // anything.
        SkillEvent::Started => match state {
            SkillRunState::Running { .. } => state.clone(),
            _ => SkillRunState::Running { messages: Vec::new() },
        },

        // Only a running state accumulates. A line arriving after the
        // terminal one (or before a tap, which cannot happen but costs
        // nothing to exclude) is dropped rather than reopening a finished
        // run.
        SkillEvent::Progress { message } => match state {
            SkillRunState::Running { messages } => {
                SkillRunState::Running { messages: append(messages, message) }
            }
            _ => state.clone(),
        },

        SkillEvent::Ok { result, backend, model } => match state {
            SkillRunState::Running { messages } => SkillRunState::Done {
                messages: messages.clone(),
                note: microtask_result(result).map(|result| result.note).unwrap_or_default(),
                backend: backend.clone(),
                model: model.clone(),
            },
            _ => state.clone(),
        },

        SkillEvent::Failed { error, backend, model, answered } => match state {
            SkillRunState::Running { messages } => SkillRunState::Declined {
                messages: messages.clone(),
                reason: error.clone(),
                backend: backend.clone(),
                model: model.clone(),
                answered: *answered,
            },
            _ => state.clone(),
        },

        // Dropped from the narration, and deliberately NOT terminal: a
        // stream that emits one garbage line has not ended. If the stream
        // then ends with nothing terminal at all, the transport synthesizes
        // the terminal event carrying [`super::decline::NO_TERMINAL_LINE`]
        // — this reducer never has to guess that a run is over.
        SkillEvent::Unreadable => state.clone(),
    }
}

/// The runner heartbeats `"still running"` every 20s, and an op can repeat
/// a line of its own; a narration that stutters the same sentence adds
/// nothing a reader can use. **This is the whole of heartbeat handling** —
/// there is no timer anywhere in this lane, on any platform.
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
    use serde_json::json;

    /// The cases ported from `run-state.test.ts`.
    fn drive(events: Vec<SkillEvent>) -> SkillRunState {
        drive_from(IDLE, events)
    }

    fn drive_from(state: SkillRunState, events: Vec<SkillEvent>) -> SkillRunState {
        events.iter().fold(state, |state, event| reduce_run(&state, event))
    }

    fn progress(message: &str) -> SkillEvent {
        SkillEvent::Progress { message: message.to_string() }
    }

    fn messages_of(state: &SkillRunState) -> Vec<String> {
        match state {
            SkillRunState::Running { messages }
            | SkillRunState::Done { messages, .. }
            | SkillRunState::Declined { messages, .. } => messages.clone(),
            SkillRunState::Idle => Vec::new(),
        }
    }

    #[test]
    fn a_tap_starts_an_empty_running_state() {
        let started = reduce_run(&IDLE, &SkillEvent::Started);
        assert_eq!(started, SkillRunState::Running { messages: Vec::new() });
        assert!(is_running(&started));
    }

    /// The duplicate-tap rule lives here, not only in a button's
    /// `disabled` — so a unit test AND a component test each prove it.
    #[test]
    fn a_second_start_while_streaming_leaves_the_state_untouched() {
        let running = drive(vec![SkillEvent::Started, progress("one")]);
        assert_eq!(reduce_run(&running, &SkillEvent::Started), running);
    }

    #[test]
    fn progress_accumulates_in_order() {
        assert_eq!(
            drive(vec![SkillEvent::Started, progress("a"), progress("b")]),
            SkillRunState::Running { messages: vec!["a".to_string(), "b".to_string()] },
        );
    }

    /// The runner heartbeats `"still running"` every 20s.
    #[test]
    fn consecutive_duplicate_messages_collapse() {
        let state = drive(vec![
            SkillEvent::Started,
            progress("still running"),
            progress("still running"),
            progress("still running"),
            progress("writing"),
        ]);
        assert_eq!(
            state,
            SkillRunState::Running {
                messages: vec!["still running".to_string(), "writing".to_string()],
            },
        );
    }

    #[test]
    fn a_non_consecutive_repeat_is_kept_it_is_a_real_second_occurrence() {
        let state = drive(vec![SkillEvent::Started, progress("a"), progress("b"), progress("a")]);
        assert_eq!(messages_of(&state), vec!["a".to_string(), "b".to_string(), "a".to_string()]);
    }

    #[test]
    fn an_ok_terminal_keeps_the_narration_and_takes_the_note_and_the_stamp() {
        let state = drive(vec![
            SkillEvent::Started,
            progress("reading"),
            SkillEvent::Ok {
                result: json!({"steps": ["put on music"], "note": "kept 2 ticked steps"}),
                backend: Some("a".to_string()),
                model: Some("m".to_string()),
            },
        ]);
        assert_eq!(
            state,
            SkillRunState::Done {
                messages: vec!["reading".to_string()],
                note: "kept 2 ticked steps".to_string(),
                backend: Some("a".to_string()),
                model: Some("m".to_string()),
            },
        );
    }

    #[test]
    fn an_ok_terminal_whose_result_is_not_the_schemas_shape_still_completes() {
        let state = drive(vec![
            SkillEvent::Started,
            SkillEvent::Ok { result: json!("surprise"), backend: None, model: None },
        ]);
        assert!(matches!(state, SkillRunState::Done { ref note, .. } if note.is_empty()));
    }

    #[test]
    fn a_failed_terminal_carries_the_seams_words_verbatim() {
        let reason = "This item already has 4 unticked steps. Re-run with replace to rewrite them.";
        let state = drive(vec![
            SkillEvent::Started,
            SkillEvent::Failed {
                error: reason.to_string(),
                backend: None,
                model: None,
                answered: false,
            },
        ]);
        assert!(matches!(state, SkillRunState::Declined { reason: ref r, .. } if r == reason));
    }

    /// A stream that emits one garbage line has not ended.
    #[test]
    fn an_unreadable_line_drops_from_the_narration_without_terminating() {
        let state =
            drive(vec![SkillEvent::Started, progress("a"), SkillEvent::Unreadable, progress("b")]);
        assert_eq!(
            state,
            SkillRunState::Running { messages: vec!["a".to_string(), "b".to_string()] },
        );
    }

    #[test]
    fn nothing_reopens_a_finished_run() {
        let done = drive(vec![
            SkillEvent::Started,
            SkillEvent::Ok { result: Value::Null, backend: None, model: None },
        ]);
        for event in [
            progress("late"),
            SkillEvent::Failed {
                error: "late".to_string(),
                backend: None,
                model: None,
                answered: false,
            },
            SkillEvent::Ok { result: Value::Null, backend: None, model: None },
        ] {
            assert_eq!(reduce_run(&done, &event), done);
        }
    }

    #[test]
    fn an_event_before_any_tap_is_ignored() {
        assert_eq!(reduce_run(&IDLE, &progress("x")), IDLE);
    }

    #[test]
    fn stamp_label_joins_the_backend_and_the_model() {
        let state = drive(vec![
            SkillEvent::Started,
            SkillEvent::Ok {
                result: Value::Null,
                backend: Some("a".to_string()),
                model: Some("m".to_string()),
            },
        ]);
        assert_eq!(stamp_label(&state).as_deref(), Some("a · m"));
    }

    #[test]
    fn stamp_label_names_the_backend_alone_when_no_model_was_reported() {
        let state = drive(vec![
            SkillEvent::Started,
            SkillEvent::Ok { result: Value::Null, backend: Some("a".to_string()), model: None },
        ]);
        assert_eq!(stamp_label(&state).as_deref(), Some("a"));
    }

    /// An unstamped envelope means nothing was attempted — so there is
    /// nothing to render, and nothing is invented here.
    #[test]
    fn stamp_label_is_none_when_the_envelope_named_no_backend() {
        let state = drive(vec![
            SkillEvent::Started,
            SkillEvent::Failed {
                error: "Cloud runner unreachable.".to_string(),
                backend: None,
                model: None,
                answered: false,
            },
        ]);
        assert_eq!(stamp_label(&state), None);
    }

    #[test]
    fn stamp_label_is_none_while_idle_and_while_running() {
        assert_eq!(stamp_label(&IDLE), None);
        assert_eq!(stamp_label(&drive(vec![SkillEvent::Started])), None);
    }

    /// The two states the seam round-trips must survive their own JSON —
    /// the web's wrapper compares the serialized forms to decide whether
    /// the reducer changed anything, so a phase that does not deserialize
    /// back to itself would silently break the no-op rule above.
    #[test]
    fn every_phase_round_trips_through_its_json() {
        let states = vec![
            IDLE,
            drive(vec![SkillEvent::Started, progress("a")]),
            drive(vec![
                SkillEvent::Started,
                SkillEvent::Ok {
                    result: json!({"steps": [], "note": "n"}),
                    backend: Some("a".to_string()),
                    model: None,
                },
            ]),
            drive(vec![
                SkillEvent::Started,
                SkillEvent::Failed {
                    error: "no".to_string(),
                    backend: None,
                    model: None,
                    answered: true,
                },
            ]),
        ];
        for state in states {
            let text = serde_json::to_string(&state).unwrap();
            let back: SkillRunState = serde_json::from_str(&text).unwrap();
            assert_eq!(back, state, "{text}");
        }
    }

    /// A `failed` event with no `answered` key — every event the transport
    /// builds straight from a line — reads as unrouted.
    #[test]
    fn an_event_without_answered_deserializes_as_unrouted() {
        let event: SkillEvent =
            serde_json::from_str(r#"{"kind":"failed","error":"x","backend":null,"model":null}"#)
                .unwrap();
        assert_eq!(
            event,
            SkillEvent::Failed {
                error: "x".to_string(),
                backend: None,
                model: None,
                answered: false,
            },
        );
    }
}
