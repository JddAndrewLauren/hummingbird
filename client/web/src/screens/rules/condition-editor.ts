import type { ConditionDTO, FieldTypeName } from "../../store/protocol";
import type { OperatorName } from "./operators";
import { defaultOperatorFor, legalOperators } from "./operators";

// The value widget cascade, per the Agent Brief: "pick a kind, then a
// field from that kind's declared descriptors, then only the operators
// legal for that field's type, then the value widget the type implies."

export type ValueWidget = "chips" | "duration" | "datetime" | "boolean" | "number" | "text";

/** The value widget one condition row offers, given the field it targets
 * and the operator selected on it.
 *
 * Priority order matters and is deliberate: `deadline` gets a date/time
 * picker even though its only legal operators are the relative-time ones
 * (ADR-0013: `deadline within_next '2h'`) — picking a target date is the
 * more natural gesture for a deadline specifically, and the picker
 * computes the equivalent duration string underneath, so the wire value is
 * still an ordinary duration. Every *other* `within_next`/`within_last`
 * condition (e.g. `received_at within_last '10m'`) gets the plain
 * duration picker instead. */
export function widgetFor(fieldName: string, fieldType: FieldTypeName, operator: OperatorName): ValueWidget {
  if (fieldType === "string_list") {
    return "chips";
  }
  if (operator === "within_next" || operator === "within_last") {
    return fieldName === "deadline" ? "datetime" : "duration";
  }
  if (fieldType === "bool") {
    return "boolean";
  }
  if (fieldType === "number") {
    return "number";
  }
  return "text";
}

/** A fresh, legal condition on `field` — the operator defaults to the
 * first legal one for its type, `value` starts empty in the shape the
 * chosen widget expects, and `negate` starts off. */
export function newCondition(fieldName: string, fieldType: FieldTypeName): ConditionDTO {
  const op = defaultOperatorFor(fieldType);
  return {
    field: fieldName,
    op,
    value: fieldType === "string_list" ? [] : "",
    negate: false,
  };
}

/** Re-legalises `condition` after its field's type changed (a field pick
 * on the same row) — resets `op` to the new type's default when the old
 * one is no longer legal, and resets `value` to that type's empty shape.
 * Called whenever the row's field selection changes; never silently
 * leaves a condition holding an operator its new field cannot support. */
export function retypeCondition(condition: ConditionDTO, newFieldType: FieldTypeName): ConditionDTO {
  const legal = legalOperators(newFieldType);
  if ((legal as string[]).includes(condition.op)) {
    return condition;
  }
  return { ...condition, op: defaultOperatorFor(newFieldType), value: newFieldType === "string_list" ? [] : "" };
}

/** Toggles a condition's `negate` flag — the per-row "not" toggle. */
export function toggleNegate(condition: ConditionDTO): ConditionDTO {
  return { ...condition, negate: !condition.negate };
}
