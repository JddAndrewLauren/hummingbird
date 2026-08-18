// The value widget cascade, per the Agent Brief: "pick a kind, then a
// field from that kind's declared descriptors, then only the operators
// legal for that field's type, then the value widget the type implies."
// No longer implemented here: it is
// `hummingbird_core::decisions::rules::editor` (ADR-0025, #141/M4, #540).
//
// This one was not in #540's five named items and was forced by them
// anyway — the phone's create-and-edit form needs the same cascade, and
// ADR-0025 forbids Kotlin holding a per-row decision function, so a Kotlin
// copy would have been the third. See `rules/mod.rs`'s own header.
//
// `retypeCondition` still returns the caller's own object untouched when
// the condition is already legal for the new type (the Rust side answers
// "leave it alone" rather than a structurally equal copy) — see
// `decisions/seam.ts`'s wrapper.
//
// This module is kept as the import site rather than deleted so the sink
// stayed a rewire: every caller and `condition-editor.test.ts` are
// untouched.

export {
  newCondition,
  retypeCondition,
  toggleNegate,
  widgetFor,
  type ValueWidget,
} from "../../decisions/seam";
