import type { KindRegistryDTO, RuleDTO } from "../../store/protocol";
import { fieldsForKind } from "./registry";

// The Agent Brief's own words: "Surface a rule as invalid when it names a
// field its kind no longer declares. It must never silently stop firing —
// a silently-dead rule is the failure mode that erodes trust in the whole
// channel." This is a display-only read: it never blocks a save (#133's
// `validate_rule` already does that, server-side, at save time) and never
// mutates the rule — it only tells the screen to say so.

export interface RuleInvalidField {
  field: string;
}

/** Every condition field on `rule` that its `eventKind` no longer
 * declares — empty when the rule is entirely valid against the current
 * registry. Checked against [`fieldsForKind`], the exact list the editor
 * itself offers, so "invalid" here means precisely "this editor could not
 * have produced this condition today." */
export function invalidFields(rule: RuleDTO, registry: KindRegistryDTO): RuleInvalidField[] {
  const legal = new Set(fieldsForKind(registry, rule.eventKind).map((field) => field.name));
  return rule.conditions.filter((condition) => !legal.has(condition.field)).map((condition) => ({ field: condition.field }));
}

/** Whether `rule` is valid against the current registry — the boolean an
 * invalid-rule badge gates on. */
export function isRuleValid(rule: RuleDTO, registry: KindRegistryDTO): boolean {
  return invalidFields(rule, registry).length === 0;
}
