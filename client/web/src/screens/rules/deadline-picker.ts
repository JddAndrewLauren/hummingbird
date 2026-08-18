// The `deadline` date/time picker (#140, acceptance criterion 3) — one of
// the two drifts ADR-0025's M1 verdict table recorded as known debt
// (`rules/deadline-picker.ts:32`). No longer implemented here: it is
// `hummingbird_core::decisions::rules::deadline` (ADR-0025, #141/M4,
// #540).
//
// What this module re-derived was the civil-time arithmetic itself — its
// own `YYYY-MM-DDTHH:MM` formatter, its own parse, its own ± duration in
// epoch milliseconds — beside `hummingbird_domain::deadline`'s. Both
// directions are now `shift` and `minutes_until`, so the picker shares the
// day-only → `T23:59` convention with the sort key and the rule evaluator
// instead of agreeing with them by coincidence.
//
// Unchanged in meaning: the picker still writes a *duration*, not a moment
// (ADR-0013's wire value), still resolves against a caller-supplied
// `nowMs` read at the moment of the edit, and still clamps a target picked
// on the "wrong" side of now up to one minute rather than refusing to
// write.
//
// This module is kept as the import site rather than deleted so the sink
// stayed a rewire: every caller and `deadline-picker.test.ts` are
// untouched.

export {
  datetimeInputValueFromDuration,
  durationFromDatetimeInputValue,
  type DeadlineOperator,
} from "../../decisions/seam";
