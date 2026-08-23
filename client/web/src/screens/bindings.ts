import type { QuestionRosterEntry } from "../decisions/seam";
import type { BindingDTO, BindingValueDTO, QuestionSwitchDTO } from "../store/protocol";
import type { TaskBindingResult, TaskQuestionSwitchResult } from "../store/store";

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
  // Not a source binding: this one names something the homework pane
  // *draws* rather than something it reads (`bindings.rs`'s own note on the
  // key). It is here rather than in the code because it carries a passcode
  // and this repo is public.
  "homework-link": {
    label: "Homework session link",
    help: "The meeting link the homework answer offers. Kept here, not in the code — it carries a passcode.",
  },
  // Written by the OpenClaw agent (ADR-0032 part 4), not typed here in the
  // ordinary case — the editor still lets the operator hand-correct it.
  "scps-quest": {
    label: "SCPS Photo Quest",
    help: "This month's Photo Quest phrase, as \"YYYY-MM phrase\" — normally set by the agent from forwarded club email.",
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

// -- the standing-question roster (#714, ADR-0034 decision 4) ---------------
//
// The section stopped being a flat list of keys and became a list of
// *questions* with their keys nested. The relation behind that nesting is
// the core's (`decisions::questions`), reached through
// `questionRosterFromCore`; the two functions below only fold `Core::
// bindings`' answer into it, and take the roster as an argument so they can
// be tested without the wasm seam.

/** One question's row group, as the section draws it. */
export interface QuestionBindingGroup {
  /** Whether this question is asked at all (#715), or `null` when the
   * switches have not been read yet.
   *
   * `null` rather than a defaulted `true`, on `TaskState.questionSwitches`'
   * own contract: a roster that drew ten toggles from a list it had not read
   * would state a fact about the workspace, and the first one to flip on the
   * next answer would look like a bug rather than an answer arriving. */
  enabled: boolean | null;
  /** Whether an unconfirmed toggle write is overlaid on this question — the
   * same read-time fact `BindingDTO.pending` carries for a binding row. */
  pending: boolean;
  /** The wire spelling of the question — a stable key for React, never
   * shown. */
  question: string;
  label: string;
  surface: string;
  /** The rows that answer this question, in the roster's own key order.
   * Empty for most questions, and an empty group is still rendered: a
   * question nobody has to configure is a fact, not an omission. */
  rows: BindingDTO[];
  /** Keys the roster says answer this question for which the reader was
   * given no row at all.
   *
   * `Core::bindings` returns every key it knows, set or not, so in
   * production this is always empty — but the demo world hand-authors a
   * subset, and reporting a question with a *missing* row as a question
   * with *nothing to set* would be the flat opposite of true. The two
   * cases are separated here so the screen can say which one it is. */
  missing: string[];
}

/** The section's whole shape: every question, then whatever `settings` rows
 * belonged to none of them. */
export interface GroupedBindings {
  groups: QuestionBindingGroup[];
  /** Live rows no question claims — in practice the keys this build cannot
   * write (`BindingDTO.known === false`), which `Core::bindings` returns on
   * purpose so the editor shows what is really in the table. Dropping them
   * here would be the regression `bindings.rs` warns about. */
  other: BindingDTO[];
}

/** Folds `Core::bindings`' flat answer into the core's question roster.
 *
 * Never invents or hides a row: every input row lands in exactly one group
 * or in `other`, and the count is asserted by this module's own test. */
export function groupBindingsByQuestion(
  roster: readonly QuestionRosterEntry[],
  bindings: readonly BindingDTO[],
  /** `Core::question_switches`' answer (#715), or `null` before the first
   * one arrives. A question with no entry in a non-null list is `null` too —
   * "this reader was handed no switch for it", which is the same distinction
   * `missing` draws for a binding row and is exactly the demo-world
   * asymmetry that shipped a copy bug at #714. */
  switches: readonly QuestionSwitchDTO[] | null = null,
): GroupedBindings {
  const claimed = new Set<string>();
  const groups = roster.map((entry) => {
    const rows: BindingDTO[] = [];
    const missing: string[] = [];
    for (const key of entry.bindings) {
      const row = bindings.find((binding) => binding.key === key);
      if (row === undefined) {
        missing.push(key);
        continue;
      }
      claimed.add(key);
      rows.push(row);
    }
    const switchState = switches?.find((candidate) => candidate.question === entry.question);
    return {
      question: entry.question,
      label: entry.label,
      surface: entry.surface,
      enabled: switchState === undefined ? null : switchState.enabled,
      pending: switchState?.pending ?? false,
      rows,
      missing,
    };
  });
  return { groups, other: bindings.filter((binding) => !claimed.has(binding.key)) };
}

/** The message a failed toggle write should show on its own question's row,
 * or `null` when the last write was another question's or succeeded —
 * [`bindingWriteError`] verbatim for the question vocabulary, matched by
 * question so a stale failure never bleeds onto a row it did not come
 * from. */
export function questionSwitchWriteError(
  lastWrite: TaskQuestionSwitchResult | null,
  question: string,
): string | null {
  if (lastWrite === null || lastWrite.question !== question || lastWrite.kind === "ok") {
    return null;
  }
  switch (lastWrite.kind) {
    case "unknown_question":
      return "This build doesn't know that question, so it wasn't switched.";
    case "busy":
      return "The core was busy. Try again.";
    case "failed":
      return lastWrite.error ?? "That switch didn't save.";
  }
}

/** Which question a binding key answers, in the reader's words — the lookup
 * that replaced the calendar picker's hand-written *"it answers How long to
 * the next vacation"* (#714). `null` when no question claims the key, which
 * is what an unwritable row is; the caller says something else then rather
 * than naming a question that does not exist. */
export function questionLabelForBinding(
  roster: readonly QuestionRosterEntry[],
  key: string,
): string | null {
  const owner = roster.find((entry) => entry.bindings.includes(key));
  return owner === undefined ? null : owner.label;
}
