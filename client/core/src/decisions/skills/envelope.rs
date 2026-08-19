//! What one NDJSON line from the skill runner *means* (#273, sunk here from
//! `client/web/src/skills/envelope.ts` by ADR-0025 at #538).
//!
//! The wire contract is `docs/runner.md`'s: zero or more
//! `{"type":"progress","message":…}` lines, then exactly one terminal
//! `{ok, skill, result|error}` line, which past the dispatch also carries a
//! `backend`/`model` stamp. The authority's proxy (ADR-0018) synthesizes its
//! own failures in the same shape, unstamped, so there is exactly one line
//! grammar to read and no client ever branches on where a line came from.
//!
//! **`backend` and `model` are `Option<String>` and are never defaulted to
//! a literal here.** That is what makes #273's "rendered from the envelope,
//! not hardcoded at the render site" true by construction rather than by
//! review: there is no provider name anywhere in this family for a render
//! site to inherit, on any platform.
//!
//! Everything crosses both seams as JSON — the serde attributes below are
//! the wire, not a convenience. [`GrillQuestion`] is `camelCase` because
//! `grill-me`'s own schema is, and [`GrillProposal::verdict`] reuses
//! [`GrillVerdict`] rather than spelling `fog_remains` a second time.

use hummingbird_domain::GrillVerdict;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// One classified line. [`SkillLine::Unreadable`] is a line this client
/// cannot parse — dropped from the narration, never treated as terminal (a
/// stream that emits garbage mid-flight has not ended).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SkillLine {
    Progress {
        message: String,
    },
    Ok {
        /// Absent on the wire reads as `null`: the run still succeeded, and
        /// the result readers below already answer `None` for it.
        #[serde(default)]
        result: Value,
        backend: Option<String>,
        model: Option<String>,
    },
    Failed {
        error: String,
        backend: Option<String>,
        model: Option<String>,
    },
    Unreadable,
}

pub fn classify_line(text: &str) -> SkillLine {
    let Ok(Value::Object(line)) = serde_json::from_str::<Value>(text) else {
        return SkillLine::Unreadable;
    };

    if line.get("type") == Some(&Value::String("progress".to_string())) {
        return match line.get("message").and_then(Value::as_str) {
            Some(message) => SkillLine::Progress { message: message.to_string() },
            None => SkillLine::Unreadable,
        };
    }

    // `ok` must be a real boolean. A string `"true"` is a malformed line,
    // not a success — truthy-coercing it would render a failed run as a
    // finished one, which is the whole reason this is matched against
    // `Value::Bool` rather than read through anything lenient.
    match line.get("ok") {
        Some(Value::Bool(true)) => {
            let (backend, model) = stamp(&line);
            SkillLine::Ok { result: line.get("result").cloned().unwrap_or(Value::Null), backend, model }
        }
        Some(Value::Bool(false)) => match line.get("error").and_then(Value::as_str) {
            Some(error) => {
                let (backend, model) = stamp(&line);
                SkillLine::Failed { error: error.to_string(), backend, model }
            }
            None => SkillLine::Unreadable,
        },
        _ => SkillLine::Unreadable,
    }
}

/// A non-string `backend`/`model` reads as absent, exactly as a missing one
/// does — there is no third answer, and no literal to fall back to.
fn stamp(line: &Map<String, Value>) -> (Option<String>, Option<String>) {
    let read = |key: &str| line.get(key).and_then(Value::as_str).map(str::to_string);
    (read("backend"), read("model"))
}

/// What `microtask` answers against its schema
/// (`.claude/skills/microtask/schema.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MicrotaskResult {
    pub steps: Vec<String>,
    pub note: String,
}

/// The terminal line's `result`, read as `microtask`'s own schema. `None`
/// when it is not that shape — the run still succeeded, so this is not a
/// failure; there is just nothing to say about what came back beyond the
/// stamp.
///
/// The steps themselves are **not** read from here. They arrive through the
/// normal step read path, because the runner already wrote them to the
/// authority; taking them from this object would be the second source of
/// truth for steps that #273 forbids. `note` is what this is for.
pub fn microtask_result(result: &Value) -> Option<MicrotaskResult> {
    let value = result.as_object()?;
    let steps: Vec<String> = value
        .get("steps")
        .and_then(Value::as_array)
        .map(|steps| steps.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    let note = value.get("note").and_then(Value::as_str).unwrap_or("").to_string();
    if steps.is_empty() && note.is_empty() {
        return None;
    }
    Some(MicrotaskResult { steps, note })
}

/// ADR-0023's typed turn: `grill-me`'s own `question` shape
/// (`.claude/skills/grill-me/schema.json`) — 2-4 short `choices`, and free
/// text is always still a valid answer regardless of what they list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillQuestion {
    pub prompt: String,
    pub recommended_answer: String,
    pub choices: Vec<String>,
}

/// `grill-me`'s terminal `proposal` shape. `patch` is whatever item-field
/// edits the interview turned up, as an opaque object on the **write
/// path**: what travels to `Core::complete_grill`'s `applied_patch` is the
/// object whole, never a key read out of it. The one reader is the review
/// card's *render* (#595): `super::review::proposal_rows` turns it into
/// labelled rows so the person confirms words, not escaped JSON — a
/// display decision that records nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrillProposal {
    pub summary: String,
    pub verdict: GrillVerdict,
    pub patch: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum GrillTurnResult {
    Question { question: GrillQuestion },
    Proposal { proposal: GrillProposal },
}

/// The terminal line's `result`, read as `grill-me`'s own schema
/// (ADR-0023): exactly one of a `question` or a `proposal` turn, `None` for
/// anything else — including a result that names both or neither, which the
/// runner's own `oneOf` schema treats as a failed run in the first place, so
/// there is nothing more specific to say about it here than "not that
/// shape".
pub fn grill_result(result: &Value) -> Option<GrillTurnResult> {
    let value = result.as_object()?;
    match value.get("kind").and_then(Value::as_str)? {
        "question" => {
            let question: GrillQuestion = serde_json::from_value(value.get("question")?.clone()).ok()?;
            // 2-4, the schema's own bound. A one-choice question is not a
            // narrower question, it is a malformed one.
            (2..=4).contains(&question.choices.len()).then_some(GrillTurnResult::Question { question })
        }
        "proposal" => {
            let proposal: GrillProposal = serde_json::from_value(value.get("proposal")?.clone()).ok()?;
            Some(GrillTurnResult::Proposal { proposal })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The cases ported from `envelope.test.ts`, which stays green against
    /// the seam wrapper — the two suites pinning the same rule from either
    /// side of the boundary is what makes the sink a rewire, not a rewrite.
    #[test]
    fn reads_a_progress_line() {
        assert_eq!(
            classify_line(r#"{"type":"progress","message":"running skill microtask"}"#),
            SkillLine::Progress { message: "running skill microtask".to_string() },
        );
    }

    #[test]
    fn reads_a_stamped_ok_line() {
        assert_eq!(
            classify_line(
                r#"{"ok":true,"skill":"microtask","result":{"steps":[],"note":"n"},"backend":"a","model":"m"}"#
            ),
            SkillLine::Ok {
                result: json!({"steps": [], "note": "n"}),
                backend: Some("a".to_string()),
                model: Some("m".to_string()),
            },
        );
    }

    #[test]
    fn reads_a_stamped_failure_line() {
        assert_eq!(
            classify_line(r#"{"ok":false,"skill":"microtask","error":"nope","backend":"a","model":null}"#),
            SkillLine::Failed { error: "nope".to_string(), backend: Some("a".to_string()), model: None },
        );
    }

    /// An unstamped line means nothing was attempted (ADR-0018). `None` is
    /// the only honest reading, and it must never be replaced with a
    /// literal.
    #[test]
    fn an_unstamped_line_carries_none_never_a_default_name() {
        assert_eq!(
            classify_line(r#"{"ok":false,"skill":null,"error":"Cloud runner unreachable."}"#),
            SkillLine::Failed {
                error: "Cloud runner unreachable.".to_string(),
                backend: None,
                model: None,
            },
        );
    }

    #[test]
    fn a_non_string_backend_or_model_reads_as_absent() {
        assert_eq!(
            classify_line(r#"{"ok":true,"result":null,"backend":42,"model":{"id":"x"}}"#),
            SkillLine::Ok { result: Value::Null, backend: None, model: None },
        );
    }

    /// Truthy-coercing this would render a failed run as a finished one.
    #[test]
    fn a_string_true_for_ok_is_unreadable_not_a_success() {
        assert_eq!(classify_line(r#"{"ok":"true","result":{}}"#), SkillLine::Unreadable);
        assert_eq!(classify_line(r#"{"ok":1,"result":{}}"#), SkillLine::Unreadable);
    }

    #[test]
    fn garbage_a_bare_scalar_an_array_and_a_null_are_all_unreadable() {
        for text in ["not json", "42", r#""a string""#, "[1,2]", "null"] {
            assert_eq!(classify_line(text), SkillLine::Unreadable, "{text}");
        }
    }

    #[test]
    fn a_progress_line_with_no_message_string_is_unreadable() {
        assert_eq!(classify_line(r#"{"type":"progress"}"#), SkillLine::Unreadable);
    }

    #[test]
    fn an_ok_false_line_with_no_error_string_is_unreadable() {
        assert_eq!(classify_line(r#"{"ok":false,"skill":"microtask"}"#), SkillLine::Unreadable);
    }

    #[test]
    fn microtask_result_reads_the_schemas_two_fields() {
        assert_eq!(
            microtask_result(&json!({"steps": ["put on music"], "note": "kept 2"})),
            Some(MicrotaskResult { steps: vec!["put on music".to_string()], note: "kept 2".to_string() }),
        );
    }

    #[test]
    fn microtask_result_is_none_for_anything_that_is_not_the_schemas_shape() {
        for value in [json!(null), json!(42), json!("steps"), json!([]), json!({}), json!({"steps": [], "note": ""})] {
            assert_eq!(microtask_result(&value), None, "{value}");
        }
    }

    #[test]
    fn microtask_result_drops_non_string_entries_rather_than_refusing_the_whole_result() {
        assert_eq!(
            microtask_result(&json!({"steps": ["a", 2, null, "b"], "note": ""})),
            Some(MicrotaskResult { steps: vec!["a".to_string(), "b".to_string()], note: String::new() }),
        );
    }

    #[test]
    fn grill_result_reads_a_question_turn() {
        let value = json!({
            "kind": "question",
            "question": {"prompt": "Which airport?", "recommendedAnswer": "SEA", "choices": ["SEA", "PDX"]},
        });
        assert_eq!(
            grill_result(&value),
            Some(GrillTurnResult::Question {
                question: GrillQuestion {
                    prompt: "Which airport?".to_string(),
                    recommended_answer: "SEA".to_string(),
                    choices: vec!["SEA".to_string(), "PDX".to_string()],
                },
            }),
        );
    }

    #[test]
    fn grill_result_reads_a_proposal_turn() {
        let value = json!({
            "kind": "proposal",
            "proposal": {"summary": "Settled on SEA", "verdict": "resolved", "patch": {"title": "book flights"}},
        });
        let Some(GrillTurnResult::Proposal { proposal }) = grill_result(&value) else {
            panic!("expected a proposal turn");
        };
        assert_eq!(proposal.verdict, GrillVerdict::Resolved);
        assert_eq!(proposal.summary, "Settled on SEA");
        // Opaque: carried whole, never read into.
        assert_eq!(proposal.patch.get("title"), Some(&json!("book flights")));
    }

    #[test]
    fn grill_result_is_none_for_anything_that_is_not_exactly_one_of_the_two_shapes() {
        for value in [
            json!(null),
            json!(42),
            json!("question"),
            json!([]),
            json!({}),
            json!({"kind": "question"}),
            json!({"kind": "question", "question": {"prompt": "p", "recommendedAnswer": "r", "choices": ["only one"]}}),
            json!({"kind": "proposal"}),
            json!({"kind": "proposal", "proposal": {"summary": "s", "verdict": "not-a-verdict", "patch": {}}}),
            json!({"kind": "something-else", "question": {"prompt": "p", "recommendedAnswer": "r", "choices": ["a", "b"]}}),
        ] {
            assert_eq!(grill_result(&value), None, "{value}");
        }
    }

    /// The seam hands JS `JSON.parse`-able text, so the serialized spelling
    /// of a line is part of the contract, not an implementation detail.
    #[test]
    fn a_line_serializes_under_its_kind_tag() {
        assert_eq!(
            serde_json::to_string(&SkillLine::Unreadable).unwrap(),
            r#"{"kind":"unreadable"}"#,
        );
        assert_eq!(
            serde_json::to_string(&SkillLine::Progress { message: "a".to_string() }).unwrap(),
            r#"{"kind":"progress","message":"a"}"#,
        );
    }
}
