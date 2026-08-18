//! The decisions ADR-0025 sinks out of the per-client shells: the small
//! pure predicates and derivations every client has to agree on, held once
//! here instead of once per platform.
//!
//! **What belongs here.** A decision, not a rendering. The test is whether
//! two clients disagreeing about it would be a *bug*: an Android build that
//! accepted a whitespace-only capture the web refuses is a bug, so
//! [`capture::can_submit_capture`] lives here; an Android build that draws
//! the urgency band in a different colour is not, so the colour stays in
//! the shell. ADR-0025's verdict table is the running record of where each
//! module fell and why.
//!
//! **Everything here is pure and clock-free.** No I/O, no storage, no
//! `SystemTime` — "now" arrives as an argument, exactly as [`crate::rank`]
//! already does it. That is not an aesthetic: the web reaches these
//! functions through a *second, main-thread* instantiation of the wasm
//! module (`client/web/src/decisions/seam.ts`), and ADR-0010's one-core-per-
//! origin rule is only untouched because nothing in this module constructs
//! a [`crate::Core`], opens storage, or holds a queue. A function added
//! here that needs any of those does not belong here — it belongs behind
//! the SharedWorker's existing request protocol.

pub mod actions;
pub mod capture;
pub mod frontier;
pub mod notification;
pub mod panes;
pub mod queue;
pub mod rules;
pub mod settings;
pub mod skills;
pub mod urgency;
pub mod vocabulary;

pub use actions::{applied_stage, available_actions, can_grill, can_mark_done, grill_button_label};
pub use capture::can_submit_capture;
pub use notification::{notification_tap_target, TapTarget};
