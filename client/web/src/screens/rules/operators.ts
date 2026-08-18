// The ADR-0013 operator vocabulary and its type-gating table. The table
// itself no longer lives here: it is
// `hummingbird_core::decisions::rules::operators` (ADR-0025, #141/M4,
// #540), which derives every answer from
// `hummingbird_rules_engine::Operator::is_legal_for` — the exact function
// the authority evaluates with.
//
// This module's own header used to ask for the two tables to be "kept
// byte-identical," with nothing mechanical connecting them: the kind
// registry export carries field types, never legal operators, so a drift
// was silent on both sides. There is now one table, in one language, and
// `ffi-web/src/decisions.rs`'s `the_operator_binding_is_the_core_rule_verbatim`
// pins the crossing.
//
// `OPERATOR_LABELS` stays here, deliberately: it is display copy, not a
// decision — two clients wording "is within the next" differently is a
// difference, not a bug (ADR-0025's own test for what belongs in core).
//
// This module is kept as the import site rather than deleted so the sink
// stayed a rewire: every caller and `operators.test.ts` are untouched, the
// same pattern `capture-validation.ts` established at M1-1.

import type { OperatorName } from "../../decisions/seam";

export { defaultOperatorFor, legalOperators, type OperatorName } from "../../decisions/seam";

export const OPERATOR_LABELS: Record<OperatorName, string> = {
  eq: "is",
  contains: "contains",
  gt: "is greater than",
  lt: "is less than",
  is: "is",
  within_next: "is within the next",
  within_last: "was within the last",
};
