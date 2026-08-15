// #357: the "triage process" queue (CONTEXT.md — the pair of pre-action
// stages, Triage and Grilling, together) as ONE combined, ordered read. No
// longer implemented here — it is
// `hummingbird_core::decisions::queue::triage_process_queue` (ADR-0025,
// #141/M1-3), reached through the main-thread wasm seam, so the Triage
// screen, Now's collapsible triage area and whatever renders the counts
// share one Rust-owned fact instead of three TS call sites that could
// drift.
//
// This module is kept as the import site rather than deleted so the sink
// stayed a rewire: every caller (`FrontierColumns.tsx`, `TriageScreen.tsx`)
// and `triage-process-order.test.ts` are untouched, the same pattern
// `capture-validation.ts` established at M1-1.

export { triageProcessQueue, type TriageProcessQueue } from "../decisions/seam";
