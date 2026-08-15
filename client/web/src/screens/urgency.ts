// Urgency banding and the deadline-field grammar (`isValidDeadline` /
// `isValidScheduledDate` / `deadlineSortKey`) no longer live here. They are
// `hummingbird_core::decisions::urgency` (ADR-0025, #141/M1-2), reached
// through the main-thread wasm seam — CONTEXT.md's "computed by consumers
// at read time over the mirror, never a stored class" is still exactly
// true, but the computation itself is now one function shared by the web
// and Android rather than a TS copy that could drift from a Kotlin one.
//
// This module is kept as the import site rather than deleted so the sink
// stayed a rewire: every caller (`ItemRow`, `frontier-facets.ts`,
// `capture-meta.ts`, `triage-form.ts`) and `urgency.test.ts` are untouched,
// which is what makes the unchanged component tests a regression proof —
// the same pattern `capture-validation.ts` established at M1-1.

export {
  computeUrgency,
  deadlineSortKey,
  isValidDeadline,
  isValidScheduledDate,
  type Urgency,
} from "../decisions/seam";
