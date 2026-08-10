//! `hummingbird-rules-engine`: fire-time evaluation of ADR-0013's rule
//! condition vocabulary over the Event shape (#133).
//!
//! **Evaluation semantics.** Conditions within a rule are ANDed; rules are
//! ORed across a rule set — expressed here by [`evaluate_rules`] returning
//! every rule's individual outcome rather than short-circuiting, so the
//! caller collects the OR. Evaluation is fire-time: it reads the event and
//! the rule as they are right now and never mutates an existing record — a
//! [`Verdict`] is a severity to stamp on a minted alert plus a tier to
//! stamp on a delivery, nothing more.
//!
//! The Event core and the kind registry it evaluates rules against live in
//! `hummingbird-domain` (`hummingbird_domain::event`) rather than here —
//! ADR-0013 calls the registry "a `domain` artifact," so both this engine
//! and #140's rules UI (via [`hummingbird_domain::kind_registry_json`])
//! read the identical definition; a kind cannot be added to one without
//! the other.
//!
//! Out of scope here, per the Agent Brief: fetching events from any source
//! (#135-137), the periodic item sweep (#138), sending (#139), and
//! rendering (#140). This crate only answers "does this rule match this
//! event, and if so with what verdict."

mod eval;
mod operator;

pub use eval::{evaluate_rule, evaluate_rules, validate_rule, RuleOutcome, RuleProblem, Verdict};
pub use operator::Operator;
