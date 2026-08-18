//! What a skill run *means*: the decision half of the runner lane
//! (#538/M4), sunk out of `client/web/src/skills/` by ADR-0025 so the phone
//! can drive `POST /api/skills/run` without a second copy of any of it.
//!
//! **What is here and what is not.** Here: what an NDJSON line is
//! ([`envelope`]), how a run's state advances ([`run`], [`grill`]), what a
//! decline says ([`decline`]), and what the request body's bytes are
//! ([`args`]). Not here, and deliberately per-client: the transport itself,
//! and the line splitting that feeds this family — each platform's stream
//! reader owns that (the web's `TextDecoder`, okio's `readUtf8Line` on
//! Android), because a byte-level reader is a platform fact, not a
//! decision. ADR-0025's verdict table carries that row.
//!
//! Everything obeys [`super`]'s two rules: a decision not a rendering, and
//! pure and clock-free. The second one is load-bearing twice over here —
//! the runner heartbeats every 20s and this family answers that with
//! *de-duplication in the reducer*, not a timer, so #273's "introduces no
//! new timer" survives the port to a platform where a timer would be the
//! obvious move.
//!
//! **No provider or model name appears anywhere in this family.** The stamp
//! is read off the envelope or it is `None`; that is what makes #273's "not
//! hardcoded at the render site" a property of the code on every client at
//! once rather than a review note per shell.

pub mod affordance;
pub mod args;
pub mod backend;
pub mod decline;
pub mod envelope;
pub mod grill;
pub mod review;
pub mod run;

pub use affordance::{live_undone_steps, microtask_affordance, MicrotaskAffordance};
pub use args::{format_grill_transcript, grill_run_body, microtask_run_body, GrillTurn, MicrotaskRunInput};
pub use backend::{declined_backend_fallback, fallback_backend_id, resolve_backend_selection, AUTO_SELECTION};
pub use decline::{decline_for_response, decline_for_transport, NO_TERMINAL_LINE, NO_TOKEN};
pub use envelope::{
    classify_line, grill_result, microtask_result, GrillProposal, GrillQuestion, GrillTurnResult,
    MicrotaskResult, SkillLine,
};
pub use grill::{reduce_grill_turn, GrillTurnState, OUTSIDE_SCHEMA};
pub use review::{demotes_from_frontier, plan_replacement_label, would_strand_plan, FRONTIER_DEMOTION_WARNING};
pub use run::{is_running, reduce_run, stamp_label, SkillEvent, SkillRunState};
