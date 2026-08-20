// S13/#111's triage form logic: what one triage-inbox item's edit draft looks
// like, and how it becomes the wire's `TriageEdits` — a pure module (no React,
// no worker) so the rules below are unit-testable without mounting anything,
// the same split `item-actions.ts`/`capture-validation.ts` already use for
// their own screen logic.
//
// The draft is **seeded from the item** (`draftFromItem`), not left blank, and
// that is the whole design: the expanded row is an editor showing what the item
// actually holds, so a field's meaning is "this is the value" rather than the
// old "blank means unchanged" sentinel. Two things follow, and neither was
// possible before:
//
//   * every field of an item can be edited here, not just the five the form
//     started with — because a value being shown is what makes it editable;
//   * a value can be REMOVED, since a field emptied against a seeded draft is
//     unambiguously a clear. `buildTriageEdits` sends that as an explicit
//     `null` (`TriageEdits`'s own contract), and the field-by-field diff
//     against the item is what keeps an untouched field out of the mutation
//     entirely.

import type { ProjectDTO, TaskItemDTO, TriageEdits } from "../store/protocol";
import type { TaskProjectResult } from "../store/store";
import { captureMetaProblems } from "../decisions/seam";

/** One triage-inbox item's editable draft. Every field is a string because
 * that is what an `<input>`/`<select>` controls — including `priority`, whose
 * `items.priority` encoding is an integer (`priority.ts` owns every reading of
 * that number) but whose control hands back `"0"`..`"4"`. */
export interface TriageDraft {
  title: string;
  description: string;
  projectId: string;
  size: "quick" | "normal" | "deep" | "";
  energy: "low" | "medium" | "high" | "";
  context: string;
  priority: string;
  deadline: string;
  scheduledDate: string;
}

/** A draft for an item with nothing set — the shape's resting value, used by a
 * row whose item has not arrived yet. Not a "nothing changed" marker: against a
 * real item, these empty strings would each read as a clear. */
export const EMPTY_TRIAGE_DRAFT: TriageDraft = {
  title: "",
  description: "",
  projectId: "",
  size: "",
  energy: "",
  context: "",
  priority: "0",
  deadline: "",
  scheduledDate: "",
};

/** Seeds a draft from the item's own current values. `null` becomes `""` — the
 * one representation an empty control has — which is why every "is this a
 * change?" question below is asked against the item and never against `""`
 * alone. */
export function draftFromItem(item: TaskItemDTO): TriageDraft {
  return {
    title: item.title,
    description: item.description ?? "",
    projectId: item.projectId ?? "",
    size: item.size ?? "",
    energy: item.energy ?? "",
    context: item.context ?? "",
    priority: String(item.priority),
    deadline: item.deadline ?? "",
    scheduledDate: item.scheduledDate ?? "",
  };
}

/** What the form is actually editing: the item's own values, with whatever the
 * person has typed laid over them. Only the **touched** fields are state; the
 * rest are derived on every render.
 *
 * That is deliberate, and it is what makes the hazard `bindings.ts`'s
 * `sameBindingValue` exists to patch structurally impossible here: a pull that
 * carries another device's edit to a field nobody is typing in shows through
 * immediately, because that field was never captured into state to go stale.
 * A field someone IS editing keeps their value — the one thing an editor must
 * never lose. */
export function effectiveDraft(item: TaskItemDTO, touched: Partial<TriageDraft>): TriageDraft {
  return { ...draftFromItem(item), ...touched };
}

/** Which fields of a draft cannot be sent, keyed by field so the form can put
 * each message on the control it belongs to rather than in one lump. Empty
 * means the draft is sendable.
 *
 * These are the same rules the authority answers 400 on, and which the seam
 * (`client/ffi-web/src/task_host.rs`) refuses before `Core::triage` — checked
 * here so someone sees the problem on the field instead of watching a mutation
 * dead-letter afterwards. */
export type TriageDraftProblems = Partial<Record<keyof TriageDraft, string>>;

export function triageDraftProblems(draft: TriageDraft): TriageDraftProblems {
  const problems: TriageDraftProblems = {};
  if (draft.title.trim().length === 0) {
    // `title` is `NOT NULL`: an emptied title is a mistake, not a clear, and
    // it is the one field with no "unset" state to fall back to.
    problems.title = "A title is required";
  }
  // The same `hummingbird_core::decisions::capture::capture_meta_problems`
  // call `capture-meta.ts`'s `captureMetaProblems` makes (M1-2, #500): one
  // function answering for both forms' deadline/scheduledDate fields, so
  // the two message strings this used to hand-copy cannot drift apart.
  const dateProblems = captureMetaProblems(draft.deadline, draft.scheduledDate);
  if (dateProblems.deadline !== undefined) {
    problems.deadline = dateProblems.deadline;
  }
  if (dateProblems.scheduledDate !== undefined) {
    problems.scheduledDate = dateProblems.scheduledDate;
  }
  if (!PRIORITY_VALUES.has(draft.priority)) {
    problems.priority = "Pick a priority";
  }
  return problems;
}

const PRIORITY_VALUES = new Set(["0", "1", "2", "3", "4"]);

/** Whether anything in this draft differs from the item it was seeded from —
 * what tells a bare promotion (no edit at all, still exactly one mutation)
 * apart from an edit, and what a form can disable a "nothing to send" state on.
 */
export function hasTriageEdits(draft: TriageDraft, item: TaskItemDTO): boolean {
  return Object.keys(buildTriageEdits(draft, item)).length > 0;
}

/** Builds the wire `TriageEdits` this issue's "a multi-field triage is one
 * mutation, not four" acceptance rides on: one object carrying every field
 * that actually changed against `item`, and only those.
 *
 * The three instructions `TriageEdits` documents are all reachable from here:
 * a field equal to the item's value is **omitted** ("leave it alone"), a
 * nullable field emptied against a value the item holds becomes **`null`**
 * ("clear it"), and anything else is **the new value**. Still exactly one call
 * either way; collapsing the fields into one `PATCH` is `Core::triage`'s job,
 * not this function's — this only decides WHICH fields are changes.
 *
 * `project`, since #631, is whatever `ProjectDTO` `draft.projectId` currently
 * resolves to (`null` when it resolves to none). It exists for ADR-0030
 * decision 3's copy-at-mint: when this call assigns the item into a project
 * it was not already in — `item.projectId !== project.id`, the "promotion"
 * moment the decision names — and the draft's own `context` is empty, the
 * project's `defaultContext` (when it has one) is copied onto `edits.context`
 * as the item's own value, exactly as if the human had typed it. Gated on the
 * transition (not merely "context empty, project selected") so a later save
 * that touches neither field never re-copies anything into an item already
 * sitting in the project — the acceptance's "nothing retroactive". A context
 * the human DID type always wins: `context.length === 0` is false the moment
 * they have, so nothing here ever overwrites it. An explicit CLEAR wins too —
 * gated on `!("context" in edits)` — so a human who empties a seeded context
 * and picks a project in the same save keeps the clear; the default is only
 * copied when `edits.context` is still unset. This is also entry point 3
 * of the decision ("assigning a project to an already-existing context-less
 * item") for free — detail mode's Save calls this same function.
 *
 * Callers are expected to have consulted `triageDraftProblems` first; this
 * does not re-check validity, the same trust `capture-validation.ts` is given
 * by its own caller. */
export function buildTriageEdits(
  draft: TriageDraft,
  item: TaskItemDTO,
  project: ProjectDTO | null = null,
): TriageEdits {
  const edits: TriageEdits = {};

  const title = draft.title.trim();
  if (title.length > 0 && title !== item.title) {
    edits.title = title;
  }

  const description = draft.description.trim();
  if (description !== (item.description ?? "")) {
    edits.description = description.length > 0 ? description : null;
  }

  if (draft.projectId !== (item.projectId ?? "")) {
    edits.projectId = draft.projectId.length > 0 ? draft.projectId : null;
  }

  if (draft.size !== (item.size ?? "")) {
    // `""` is this control's "Not set"; on the wire that is a `null` clear, and
    // the vocabulary itself has no empty member.
    edits.size = draft.size === "" ? null : draft.size;
  }

  if (draft.energy !== (item.energy ?? "")) {
    edits.energy = draft.energy === "" ? null : draft.energy;
  }

  const context = draft.context.trim();
  if (context !== (item.context ?? "")) {
    edits.context = context.length > 0 ? context : null;
  }

  // ADR-0030 decision 3's copy-at-mint — see this function's own doc for the
  // transition gate and why it cannot fire on an untouched save. Also gated
  // on `!("context" in edits)`: when the block above already recorded an
  // explicit clear (the human emptied a seeded context in this same save),
  // that clear is human-typed and wins — never overwritten by the default,
  // matching the sibling copy-at-mint path merged in #632.
  if (
    context.length === 0 &&
    !("context" in edits) &&
    project !== null &&
    project.defaultContext !== null &&
    item.projectId !== project.id
  ) {
    edits.context = project.defaultContext;
  }

  // A number on the wire, a string in the control. `NOT NULL`, so there is no
  // clear — "No priority" is the real value `0`, not an absence.
  const priority = Number(draft.priority);
  if (Number.isInteger(priority) && priority !== item.priority) {
    edits.priority = priority;
  }

  const deadline = draft.deadline.trim();
  if (deadline !== (item.deadline ?? "")) {
    edits.deadline = deadline.length > 0 ? deadline : null;
  }

  const scheduledDate = draft.scheduledDate.trim();
  if (scheduledDate !== (item.scheduledDate ?? "")) {
    edits.scheduledDate = scheduledDate.length > 0 ? scheduledDate : null;
  }

  return edits;
}

/** #631: the triage row's Project select carries this sentinel as its own
 * option, alongside every real project id — never sent anywhere, only
 * matched by the field's own `onChange` to switch the control from a picker
 * into the inline "new project" form. */
export const NEW_PROJECT_OPTION = "__new_project__";

/** True while an inline project create issued from a triage row is still in
 * flight — its minted id has not reached `projects` yet. Same read
 * `screens/projects/roster.ts`'s `awaitingCreate` makes for the Projects
 * grid, over the flat list a row already has on hand rather than that
 * module's `ProjectRow[]`; there is no optimistic overlay to look in either
 * way (`Core::create_project`'s own doc). */
export function awaitingProjectCreate(
  projects: ProjectDTO[],
  lastProjectWrite: TaskProjectResult | null,
): boolean {
  if (lastProjectWrite === null || lastProjectWrite.kind !== "ok") {
    return false;
  }
  const { projectId } = lastProjectWrite;
  return projectId !== null && !projects.some((project) => project.id === projectId);
}
