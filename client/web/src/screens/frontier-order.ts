// The frontier's display order (issue #108's "Ordering is a pure function
// and is unit-tested" acceptance criterion). No longer implemented here —
// it is `hummingbird_core::decisions::frontier`'s priority/deadline order
// (ADR-0025, #141/M1-3), reached through the main-thread wasm seam, which
// also replaced this rule's Rust twin over the S1/Linear-era mirror,
// `client/core/src/task/query.rs`'s own ranking step (ADR-0021 decision 1:
// one spelling of frontier order, not two).
//
// This module is kept as the import site rather than deleted so the sink
// stayed a rewire: every caller (`FrontierColumns.tsx`) and
// `frontier-order.test.ts` are untouched, the same pattern
// `capture-validation.ts` established at M1-1.

export { orderFrontier } from "../decisions/seam";
