// The editing state one item's form carries, extracted from `TriageRow` so
// that the Triage row and Now's item detail share it rather than growing two
// copies of the same three-part rule (#222's clear-on-ok is the part that is
// easy to get subtly wrong twice).
//
// Everything decidable still lives in `triage-form.ts` — which fields changed,
// what cannot be sent, what the effective draft is. This holds the React state
// those functions are called with, and nothing else.

import { useState } from "react";
import type { TaskItemDTO } from "../store/protocol";
import type { TaskTriageResult } from "../store/store";
import {
  effectiveDraft,
  triageDraftProblems,
  type TriageDraft,
  type TriageDraftProblems,
} from "./triage-form";

export interface ItemDraft {
  /** The draft as it stands: the item's own values with whatever has been
   * typed over them. */
  draft: TriageDraft;
  problems: TriageDraftProblems;
  /** True when something in the draft cannot be sent — what every submit
   * control disables on. */
  blocked: boolean;
  set: (field: keyof TriageDraft, value: string) => void;
  /** Drops every edit, back to the item's own values. The Discard control,
   * and what a caller closing an editor calls. */
  reset: () => void;
}

/** One item's editing state.
 *
 * `lastTriage` is the most recent triage result any editor got back
 * (`TaskState.lastTriage`); it is what clears the typing, and only ever on an
 * `"ok"` naming **this** item.
 *
 * Issue #222 (the capture/triage twin of PR #207's act-failure defect): the
 * typing used to clear the instant Promote was clicked, optimistically, so a
 * failed write lost the reader's edits AND said nothing about the failure. It
 * stays put — sending is the caller's job and does not touch this state — and
 * clears here, once and only once a result actually reports `"ok"` for this
 * item. The React-docs "adjusting state when a prop changes" pattern, guarded
 * on the result's own `seed` so a broadcast already observed is never
 * reprocessed, and keyed by the itemId the result carries — a success on
 * another item cannot wipe this one's still-in-flight edits.
 *
 * `onCleared` runs on that same beat, for a caller with something else to undo
 * when the write lands (item detail leaves Edit mode).
 */
export function useItemDraft(
  item: TaskItemDTO,
  lastTriage?: TaskTriageResult | null,
  onCleared?: () => void,
): ItemDraft {
  // Only what the person has typed is state — see `effectiveDraft`'s doc for
  // why the rest is derived per render rather than seeded once.
  const [touched, setTouched] = useState<Partial<TriageDraft>>({});
  const draft = effectiveDraft(item, touched);
  const problems = triageDraftProblems(draft);

  const [processedTriageSeed, setProcessedTriageSeed] = useState<string | null>(null);
  if (lastTriage && lastTriage.seed !== processedTriageSeed) {
    setProcessedTriageSeed(lastTriage.seed);
    if (lastTriage.kind === "ok" && lastTriage.itemId === item.id) {
      setTouched({});
      onCleared?.();
    }
  }

  return {
    draft,
    problems,
    blocked: Object.keys(problems).length > 0,
    set: (field, value) => setTouched((current) => ({ ...current, [field]: value })),
    reset: () => setTouched({}),
  };
}
