// The invalid-rule read — the Agent Brief's own words: "Surface a rule as
// invalid when it names a field its kind no longer declares. It must never
// silently stop firing — a silently-dead rule is the failure mode that
// erodes trust in the whole channel." No longer implemented here: it is
// `hummingbird_core::decisions::rules::validity` (ADR-0025, #141/M4,
// #540), so the phone and the browser cannot disagree about which rules
// they warn about.
//
// Still a display-only read, unchanged: it never blocks a save (#133's
// `validate_rule` already does that, server-side, at save time) and never
// mutates the rule.
//
// This module is kept as the import site rather than deleted so the sink
// stayed a rewire: every caller and `validity.test.ts` are untouched.

export { invalidFields, isRuleValid, type RuleInvalidField } from "../../decisions/seam";
