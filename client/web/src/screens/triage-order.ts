// Triage's display order (issue #110/S12's "three captures then
// reconnecting produces three Triage items in order" acceptance
// criterion). No longer implemented here — it is
// `hummingbird_core::decisions::queue::order_triage` (ADR-0025, #141/M1-3),
// reached through the main-thread wasm seam, the "Now/Triage membership +
// order" half of the sink `triage-process-order.ts` shares.
//
// This module is kept as the import site rather than deleted so the sink
// stayed a rewire: every caller (`triage-process-order.ts`) and
// `triage-order.test.ts` are untouched, the same pattern
// `capture-validation.ts` established at M1-1.

export { orderTriage } from "../decisions/seam";
