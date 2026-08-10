import type { BindingDTO, BindingValueDTO } from "../store/protocol";
import type { TaskBindingResult } from "../store/store";

// #118's bindings editor, everything decidable: what each binding is called
// in human words, what it currently holds, and whether a draft is worth
// sending. The `.tsx` only threads React state through these — the same
// pure-module split every other `screens/*.ts` uses.
//
// The key vocabulary itself is NOT redefined here. `Core::bindings` returns
// every key it knows (in vocabulary order) plus every row it does not, each
// flagged with `known`; this module only decorates that answer. A TS-side
// list of keys would be a second vocabulary to keep in step with ADR-0015's
// one, and the drift would show up as a row that silently stops rendering.

/** The human name and one-line purpose for a binding this build knows.
 * Looked up by key with a fallback (below), never used as the source of
 * which keys exist. */
export interface BindingCopy {
  label: string;
  help: string;
}

const BINDING_COPY: Record<string, BindingCopy> = {
  "race-series": {
    label: "Race series",
    help: "Which series the next-race answer follows.",
  },
  "trips-calendar": {
    label: "Trips calendar",
    help: "The calendar holding trips — the vacation countdown reads its next event.",
  },
  "city-waste-page": {
    label: "Waste schedule page",
    help: "The council page the collection schedule is read from.",
  },
};

/** Copy for one binding row. A key this build cannot write still gets a row
 * — it is really in the table — so it falls back to its raw key and says
 * plainly why it is read-only, rather than being dressed up as a binding
 * this build understands. */
export function bindingCopy(binding: BindingDTO): BindingCopy {
  const known = BINDING_COPY[binding.key];
  if (known !== undefined) {
    return known;
  }
  return {
    label: binding.key,
    help: "Set by a newer version of the app. This build can show it, but not change it.",
  };
}

/** What a binding's value should read as on screen. Three states in, three
 * distinct readings out — an unset binding must never render as an empty
 * string beside a set one, which is exactly the collapse the tagged union
 * exists to prevent. */
export function bindingValueLabel(value: BindingValueDTO): string {
  switch (value.state) {
    case "unset":
      return "Not set";
    case "text":
      return value.text;
    case "other":
      return `Not a text value: ${value.raw}`;
  }
}

/** The text a row's input starts at: the current value when it is text, and
 * an empty field otherwise. Deliberately empty for `"other"` too — a value
 * this editor cannot express must not be pre-loaded into a field whose
 * submit would overwrite it with a mangled string. */
export function bindingDraftSeed(value: BindingValueDTO): string {
  return value.state === "text" ? value.text : "";
}

/** Whether two reads of a binding's value are the same fact.
 *
 * The row's input is seeded from the value once and then owned by the
 * typist, so something has to notice when the value underneath it changes —
 * another device's edit arriving on the next pull, or this device's own
 * write confirming. Without it the field keeps showing the old value while
 * the label above it shows the new one, and Save is enabled to push the
 * stale text straight back over what just arrived (#118 review finding). */
export function sameBindingValue(a: BindingValueDTO, b: BindingValueDTO): boolean {
  if (a.state !== b.state) {
    return false;
  }
  if (a.state === "text" && b.state === "text") {
    return a.text === b.text;
  }
  if (a.state === "other" && b.state === "other") {
    return a.raw === b.raw;
  }
  return true;
}

/** The message a failed binding write should show on its own row, or `null`
 * when the last write was someone else's row or succeeded.
 *
 * Every non-`ok` outcome gets words: an enqueue that failed, a core that was
 * busy and a key this build cannot write are all "Save appeared to do
 * nothing" without one. Matched by key so a stale failure from a DIFFERENT
 * binding never bleeds onto this row — the same rule `NowScreen`'s act error
 * follows for item ids. */
export function bindingWriteError(
  lastWrite: TaskBindingResult | null,
  key: string,
): string | null {
  if (lastWrite === null || lastWrite.key !== key || lastWrite.kind === "ok") {
    return null;
  }
  switch (lastWrite.kind) {
    case "unknown_key":
      return "This build doesn't know that binding, so it wasn't saved.";
    case "busy":
      return "The core was busy. Try saving again.";
    case "failed":
      return lastWrite.error ?? "That binding didn't save.";
  }
}

/** Whether a drafted binding is worth sending.
 *
 * Refused here rather than in the core, for the same reason
 * `capture-validation.ts` refuses an empty capture: `Core::set_binding` has
 * no opinion of its own and would enqueue whatever it is handed — and in a
 * table with no DELETE, a binding blanked by a stray keystroke cannot be
 * removed, only overwritten again. A draft equal to what is already stored
 * is also refused: it would be a CAS write whose only effect is a version
 * bump, and every other view would re-render for nothing. */
export function canSubmitBinding(binding: BindingDTO, draft: string): boolean {
  if (!binding.known) {
    return false;
  }
  const trimmed = draft.trim();
  if (trimmed === "") {
    return false;
  }
  return !(binding.value.state === "text" && binding.value.text === trimmed);
}

/** What actually gets sent for a draft — trimmed, so the stored value and
 * the compared-against value can never differ by whitespace the operator
 * cannot see. Call only when [`canSubmitBinding`] passed. */
export function bindingSubmitValue(draft: string): string {
  return draft.trim();
}
