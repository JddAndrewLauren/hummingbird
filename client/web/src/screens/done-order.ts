// Done's order: most recently touched first. No longer implemented here —
// it is `hummingbird_core::decisions::roster::order_done` (ADR-0025,
// #141/M3, #532), reached through the main-thread wasm seam.
//
// This module is kept as the import site rather than deleted so the sink
// stayed a rewire: every caller (`DoneScreen.tsx`) and `done-order.test.ts`
// are untouched, the same pattern `triage-order.ts` established at M1-3.

export { orderDone } from "../decisions/seam";
