// S13/#111's triage form logic: what one triage-inbox item's inline edit
// draft looks like, and how it becomes the wire's `TriageEdits` — a pure
// module (no React, no worker) so the "leave a field alone unless it
// actually changed" rule is unit-testable without mounting anything, the
// same split `item-actions.ts`/`capture-validation.ts` already use for
// their own screen logic.

import type { TriageEdits } from "../store/worker-client";

/** One triage-inbox item's editable draft. Every field starts at `""`
 * ("not set" / "unchanged") rather than `null` — an `<input>`/`<select>`
 * needs a string to control, and `""` is never a legal title, project id,
 * size, energy or context value on the wire, so it is an unambiguous
 * "nothing typed here yet" sentinel. */
export interface TriageDraft {
  title: string;
  projectId: string;
  size: "quick" | "short" | "deep" | "";
  energy: "low" | "medium" | "high" | "";
  context: string;
}

export const EMPTY_TRIAGE_DRAFT: TriageDraft = {
  title: "",
  projectId: "",
  size: "",
  energy: "",
  context: "",
};

/** Builds the wire `TriageEdits` this issue's "a multi-field triage is one
 * mutation, not four" acceptance rides on: every field `draft` leaves blank
 * (or, for `title`, leaves equal to the item's current title) maps to
 * `null` — "leave this field alone" — never an empty string sent as a real
 * edit. Still exactly one call either way; the mutation itself collapsing
 * several fields into one `PATCH` is `Core::triage`'s job, not this
 * function's — this only decides WHICH fields are actually changes. */
export function buildTriageEdits(draft: TriageDraft, currentTitle: string): TriageEdits {
  const title = draft.title.trim();
  return {
    title: title.length > 0 && title !== currentTitle ? title : null,
    projectId: draft.projectId || null,
    size: draft.size || null,
    energy: draft.energy || null,
    context: draft.context || null,
  };
}
