import type { FieldTypeName } from "../../store/protocol";

// The ADR-0013 operator vocabulary and its type-gating table — the TS twin
// of `server/rules-engine/src/operator.rs`'s `Operator::is_legal_for`.
// **This table must be kept byte-identical to that one.** Nothing
// mechanical connects the two sides (the kind registry export carries
// field types, never legal operators — ADR-0013 gates those by type, not
// by field), so a drift here is silent on both sides, the same risk
// `server/city-waste/tests/contract.rs` exists to guard against for its
// own body shape. If the Rust table ever grows or narrows, this one must
// move with it.

export type OperatorName = "eq" | "contains" | "gt" | "lt" | "is" | "within_next" | "within_last";

export const OPERATOR_LABELS: Record<OperatorName, string> = {
  eq: "is",
  contains: "contains",
  gt: "is greater than",
  lt: "is less than",
  is: "is",
  within_next: "is within the next",
  within_last: "was within the last",
};

/** The operators legal for one field type — exactly `Operator::is_legal_for`.
 *
 * `"dynamic"` (`snapshot_change.value`/`.previous` only) is the one type
 * the Rust table treats as "anything goes, checked at evaluation time
 * against the concrete value" (`Operator::is_legal_for(Dynamic) == true`
 * for every operator) — but `FieldValue::dynamic_type` never resolves a
 * bare value to `Timestamp`/`Date`, so `within_next`/`within_last` can
 * never actually match one. Offering them here would be a control that
 * always no-ops, so this editor offers the *reachable* subset
 * (`hummingbird_domain::event`'s own doc: "only eq/contains/gt/lt/is
 * reachable through this mapping") rather than the literal `true` the
 * gating function returns for every operator.
 */
export function legalOperators(fieldType: FieldTypeName): OperatorName[] {
  switch (fieldType) {
    case "string":
      return ["eq", "contains"];
    case "string_list":
      return ["eq", "contains"];
    case "number":
      return ["eq", "gt", "lt"];
    case "bool":
      return ["is"];
    case "timestamp":
      return ["within_next", "within_last"];
    case "date":
      return ["within_next", "within_last"];
    case "dynamic":
      return ["eq", "contains", "gt", "lt", "is"];
  }
}

/** The operator a fresh condition on a field of `fieldType` starts at —
 * always the first of [`legalOperators`], so a newly added row is never in
 * an illegal state. */
export function defaultOperatorFor(fieldType: FieldTypeName): OperatorName {
  return legalOperators(fieldType)[0];
}
