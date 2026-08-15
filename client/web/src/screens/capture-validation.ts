// Issue #110/S12 acceptance: "An empty capture is refused client-side — a
// junk row must never be able to wedge the queue." A whitespace-only draft
// counts as empty.
//
// The rule no longer lives here. It is
// `hummingbird_core::decisions::can_submit_capture` (ADR-0025, #141/M1-1),
// reached through the main-thread wasm seam — `Core::capture` still has no
// opinion of its own on an empty title, so this is still a *pre-submit*
// decision the caller makes, but it is now one function shared by the web
// and Android rather than a TS copy that could drift from a Kotlin one.
//
// This module is kept as the import site rather than deleted so the sink
// stayed a rewire: every caller (`CaptureBox`, `TriageScreen`,
// `triage-form.ts`) and `capture-validation.test.ts` are untouched, which
// is what makes the unchanged component tests a regression proof.

export { canSubmitCapture } from "../decisions/seam";
