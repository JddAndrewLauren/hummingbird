// Now's centre column's "grouped into columns" display shape (#402,
// ADR-0021 decision 1). No longer implemented here — it is
// `hummingbird_core::decisions::frontier::{FrontierAxis, group_frontier}`
// (ADR-0025, #141/M1-3), reached through the main-thread wasm seam, for the
// same reason `frontier-order.ts` sank: two clients grouping the frontier
// must agree on the bucketing and label rules, not just on the stored
// values.
//
// This module is kept as the import site rather than deleted so the sink
// stayed a rewire: every caller (`FrontierColumns.tsx`, `frontier-prefs
// .ts`) and `frontier-columns.test.ts` are untouched, the same pattern
// `capture-validation.ts` established at M1-1.

export {
  DEFAULT_FRONTIER_AXIS,
  FRONTIER_AXES,
  groupFrontier,
  type FrontierAxis,
  type FrontierColumn,
} from "../decisions/seam";
