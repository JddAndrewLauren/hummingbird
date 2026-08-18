//! The `POST /api/skills/run` request bodies (#273 for `microtask`, #355
//! and ADR-0023 for `grill-me`; sunk here from `client/web/src/skills/`'s
//! `microtask-args.ts` and `grill-args.ts` at #538).
//!
//! **A body is produced as canonical JSON text, not as a map.** Every
//! client posts the string these functions return, byte for byte, and
//! `client/core/tests/fixtures/skills-run-bodies.json` pins the bytes from
//! all three sides. That is only a meaningful pin because serde emits
//! struct fields in declaration order: the field order below *is* the wire
//! order, a property of this code rather than of a convention someone has
//! to remember. Reordering a field here is a wire change and the fixture
//! will say so.

use serde::{Deserialize, Serialize};

use super::envelope::GrillQuestion;

/// Three rules, each of which is a bug if it goes the other way:
///
/// - **`ref` is the uuid, never `HB-<seq>`.** The runner accepts both, but
///   `seq` is nullable on `TaskItemDTO` and a locally-minted item that has
///   not synced yet has none — a `HB-null` would resolve to nothing.
/// - **`replace` is sent present-and-`true` or not at all.** A literal
///   `replace: false` is a valid boolean the runner would accept, and it
///   says the same thing as omitting it while making a bare run look like a
///   decision about rewriting.
/// - **`grain` and `model` are omitted when unset**, so the runner's
///   defaults stay the defaults rather than being re-specified here, where
///   they would drift.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrotaskRunInput {
    /// The item's uuid.
    pub item_id: String,
    /// Present-and-true only, on the rewrite gesture.
    #[serde(default)]
    pub replace: bool,
    /// SKILL.md's 1-3 grain scale. `None` leaves the runner's default (2).
    #[serde(default)]
    pub grain: Option<i64>,
    /// `None` — or the empty string, which is the web picker's "Default
    /// model" option — leaves the runner's configured model.
    #[serde(default)]
    pub model: Option<String>,
}

/// The wire body. Serialized, never handed out as a value: `args`' field
/// order is the whole point (see the module header).
#[derive(Serialize)]
struct MicrotaskRunBody<'a> {
    skill: &'static str,
    args: MicrotaskArgs<'a>,
}

#[derive(Serialize)]
struct MicrotaskArgs<'a> {
    #[serde(rename = "ref")]
    reference: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    replace: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    grain: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
}

pub fn microtask_run_body(input: &MicrotaskRunInput) -> String {
    let body = MicrotaskRunBody {
        skill: "microtask",
        args: MicrotaskArgs {
            reference: &input.item_id,
            replace: input.replace.then_some(true),
            grain: input.grain,
            model: input.model.clone().filter(|model| !model.is_empty()),
        },
    };
    serde_json::to_string(&body).expect("a run body is plain owned data and cannot fail to serialize")
}

/// One completed round: the question the model asked, and the answer given
/// to it — free text always, never constrained to `question.choices` (the
/// runner's own rule, restated in `grill-me`'s SKILL.md: "the answer is
/// always free text too").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrillTurn {
    pub question: GrillQuestion,
    pub answer: String,
}

/// `grill-me` is stateless — every request carries the whole conversation
/// so far — so this body's whole job is threading `turns` back exactly as
/// the runner requires them, never trimming or replaying a subset of what
/// actually happened.
///
/// No `model` option, unlike [`microtask_run_body`]: #355's brief asks for
/// no comforts beyond the interview itself, and nothing on any surface ever
/// sets one. Add it back only when a real caller needs it.
#[derive(Serialize)]
struct GrillRunBody<'a> {
    skill: &'static str,
    args: GrillArgs<'a>,
}

#[derive(Serialize)]
struct GrillArgs<'a> {
    #[serde(rename = "ref")]
    reference: &'a str,
    turns: &'a [GrillTurn],
}

pub fn grill_run_body(reference: &str, turns: &[GrillTurn]) -> String {
    let body = GrillRunBody { skill: "grill-me", args: GrillArgs { reference, turns } };
    serde_json::to_string(&body).expect("a run body is plain owned data and cannot fail to serialize")
}

/// The plain-text record `Core::complete_grill`'s `GrillCompletion.transcript`
/// carries (ADR-0023 decision 2: "the transcript that produced it"). Opaque
/// on every client's own read path — nothing parses it back — so a readable
/// Q/A listing is exactly as good as any other encoding, and far easier for
/// a human to read back on a review card. No turns reads as the empty
/// string, never a placeholder sentence.
pub fn format_grill_transcript(turns: &[GrillTurn]) -> String {
    turns
        .iter()
        .map(|turn| format!("Q: {}\nA: {}", turn.question.prompt, turn.answer))
        .collect::<Vec<String>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cases ported from `microtask-args.test.ts` and
    /// `grill-args.test.ts`, asserted against the wire *text* rather than a
    /// map — which is the sink's one deliberate strengthening of the rule,
    /// since the text is what the byte-identity fixture pins.
    fn input(item_id: &str) -> MicrotaskRunInput {
        MicrotaskRunInput { item_id: item_id.to_string(), ..MicrotaskRunInput::default() }
    }

    #[test]
    fn ref_is_the_uuid_never_hb_seq_and_a_bare_run_sends_nothing_else() {
        assert_eq!(
            microtask_run_body(&input("8f2c-uuid")),
            r#"{"skill":"microtask","args":{"ref":"8f2c-uuid"}}"#,
        );
    }

    /// A literal `replace: false` is a valid boolean the runner accepts, and
    /// it says the same thing as omitting it while making a bare run look
    /// like a decision about rewriting.
    #[test]
    fn replace_is_present_and_true_or_absent_never_false() {
        assert_eq!(
            microtask_run_body(&MicrotaskRunInput { replace: true, ..input("i") }),
            r#"{"skill":"microtask","args":{"ref":"i","replace":true}}"#,
        );
        assert!(!microtask_run_body(&MicrotaskRunInput { replace: false, ..input("i") })
            .contains("replace"));
    }

    #[test]
    fn grain_and_model_ride_along_when_set_in_declaration_order() {
        assert_eq!(
            microtask_run_body(&MicrotaskRunInput {
                replace: true,
                grain: Some(3),
                model: Some("m".to_string()),
                ..input("i")
            }),
            r#"{"skill":"microtask","args":{"ref":"i","replace":true,"grain":3,"model":"m"}}"#,
        );
    }

    /// The empty value is the web picker's "Default model" option — omitting
    /// the key is what leaves the runner's own default in place rather than
    /// naming it twice, here and there.
    #[test]
    fn an_empty_model_omits_the_key_entirely() {
        assert!(!microtask_run_body(&MicrotaskRunInput { model: Some(String::new()), ..input("i") })
            .contains("model"));
    }

    fn turn(prompt: &str, answer: &str) -> GrillTurn {
        GrillTurn {
            question: GrillQuestion {
                prompt: prompt.to_string(),
                recommended_answer: "r".to_string(),
                choices: vec!["a".to_string(), "b".to_string()],
            },
            answer: answer.to_string(),
        }
    }

    #[test]
    fn an_interview_opens_with_an_empty_turns_array() {
        assert_eq!(
            grill_run_body("8f2c-uuid", &[]),
            r#"{"skill":"grill-me","args":{"ref":"8f2c-uuid","turns":[]}}"#,
        );
    }

    #[test]
    fn the_whole_conversation_is_threaded_back_never_a_trimmed_subset() {
        let turns = vec![turn("Which airport?", "SEA"), turn("Which dates?", "September")];
        let body = grill_run_body("i", &turns);
        assert!(body.contains("Which airport?"), "{body}");
        assert!(body.contains("Which dates?"), "{body}");
        assert_eq!(
            body,
            r#"{"skill":"grill-me","args":{"ref":"i","turns":[{"question":{"prompt":"Which airport?","recommendedAnswer":"r","choices":["a","b"]},"answer":"SEA"},{"question":{"prompt":"Which dates?","recommendedAnswer":"r","choices":["a","b"]},"answer":"September"}]}}"#,
        );
    }

    #[test]
    fn a_grill_body_carries_no_model_option_the_args_are_exactly_ref_and_turns() {
        let value: serde_json::Value = serde_json::from_str(&grill_run_body("i", &[])).unwrap();
        let args = value["args"].as_object().unwrap();
        assert_eq!(args.keys().collect::<Vec<&String>>(), vec!["ref", "turns"]);
    }

    #[test]
    fn a_transcript_of_no_turns_is_the_empty_string() {
        assert_eq!(format_grill_transcript(&[]), "");
    }

    #[test]
    fn a_transcript_lists_each_round_as_a_q_a_pair_in_order() {
        let turns = vec![
            turn("Which airport?", "SEA"),
            turn("Which dates?", "the third week of September"),
        ];
        assert_eq!(
            format_grill_transcript(&turns),
            "Q: Which airport?\nA: SEA\n\nQ: Which dates?\nA: the third week of September",
        );
    }
}
