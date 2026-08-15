// Splitting a deadline into the two controls that edit it, and joining
// them back into the one string the wire carries. No longer implemented
// here — it is `hummingbird_core::decisions::urgency::{split_deadline,
// join_deadline}` (ADR-0025, #141/M1-2), reached through the main-thread
// wasm seam, for the same reason `urgency.ts` sank: two clients editing the
// same field must agree on the split, not just on the stored value.
//
// **Not `rules/deadline-picker.ts`.** That module turns a picked date into
// a *duration* for a rule's lead time; this one is about the item's own
// deadline value, which is an absolute civil date-time. Same words,
// different wire.
//
// **Malformed values still pass straight through** — the Rust
// implementation keeps that rule (see `urgency.rs`'s `split_deadline`):
// an item captured before this existed — or by a skill, or by hand — may
// carry free text in `deadline`, and a picker that silently emptied the
// field on load would delete it the moment anything else on the form was
// saved.
//
// This module is kept as the import site rather than deleted so the sink
// stayed a rewire: every caller and `deadline-parts.test.ts` are
// untouched, the same pattern `capture-validation.ts` established at M1-1.

export { joinDeadline, splitDeadline, type DeadlineParts } from "../decisions/seam";
