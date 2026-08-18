//! The rules-editor decision set (ADR-0025's M4 sink, #540): which
//! operators a field type may carry, what a duration literal means, which
//! conditions a rule's kind still declares, what a `deadline` picker writes,
//! which widget a condition row offers, and what a draft rule would have
//! matched.
//!
//! **What this retires.** ADR-0025's M1 verdict table recorded two known
//! drifts as debt — `client/web/src/screens/rules/backtest.ts:52` and
//! `rules/deadline-picker.ts:32` — where each module privately re-derived
//! the deadline reading: the day-only → `T23:59` resolution ADR-0013 owns,
//! and the duration ± civil-time arithmetic. Both are now one call into
//! [`hummingbird_domain::deadline`], reached from here. Alongside them this
//! module sinks the operator table (`operators.ts`, whose own header asked
//! to be "kept byte-identical" to `Operator::is_legal_for` with nothing
//! mechanical enforcing it), the duration grammar (`duration.ts`), the
//! validity read (`validity.ts`) and the field/widget cascade
//! (`registry.ts`, `condition-editor.ts`).
//!
//! **Why the cascade came too, though #540 names five items.**
//! [`editor::widget_for`] and [`validity::fields_for_kind`]/
//! [`validity::field_type`] are not in the issue's list, but they are
//! forced by it: the phone's create-and-edit form needs the
//! kind → field → operator → widget cascade, and ADR-0025 forbids Kotlin
//! holding a per-row decision function. A cascade decided in Kotlin would
//! be exactly the third copy this slice exists to prevent, so it is decided
//! here and shipped applied (`client/ffi-mobile`'s `RuleFormRecord`).
//!
//! **What deliberately did not sink.** Rendering: `OPERATOR_LABELS` and
//! `kindLabel` stay per-client copy, and so does the one *conversion* no
//! clock-free crate can do — epoch milliseconds ⇄ a civil wall-clock
//! string. This crate holds no tzdb (`client/core/Cargo.toml` says why at
//! length), so every function here takes "now" as an already-resolved
//! deadline-shaped string, exactly as [`crate::decisions::urgency`] does,
//! and each host converts at its own edge.

pub mod backtest;
pub mod deadline;
pub mod duration;
pub mod editor;
pub mod operators;
pub mod validity;

pub use backtest::{backtest, BacktestClock, BacktestItem, BacktestOutcome};
pub use deadline::{datetime_input_value_from_duration, duration_from_datetime_input_value};
pub use duration::{
    duration_units_for, format_duration, is_below_alarm_interval, parse_duration_ms,
};
pub use editor::{new_condition, retype_condition, toggle_negate, widget_for, ValueWidget};
pub use operators::{default_operator_for, legal_operators};
pub use validity::{
    compiled_registry, field_type, fields_for_kind, invalid_fields, is_rule_valid, KindEntry,
    KindField, KindRegistry, ALARM_INTERVAL_MS, DEFAULT_SEVERITY,
};
