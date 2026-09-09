// The Contexts section's decidable half (ADR-0038): what the section says
// and refuses before a write ever leaves the screen. The core stays the
// judge — `Core::add_context` re-checks every rule here and is the one
// answer that counts — but a field that let a blank or a duplicate travel
// to the worker and come back refused a beat later would read as a control
// that fails randomly. Pure functions over the DTOs, so `SettingsScreen.tsx`
// only threads React state; `bindings.ts`'s own split, for the same reason.

import type { ContextEntryDTO } from "../store/protocol";
import type { TaskContextEditResult } from "../store/store";

/** `hummingbird_core::rank::normalize_context`'s rule, restated for the
 * instant-feedback check only: trim, drop a leading `@`, lowercase. The
 * core's own copy is the one that decides. */
export function sameContext(a: string, b: string): boolean {
  const norm = (s: string) => s.trim().replace(/^@/, "").toLowerCase();
  return norm(a) === norm(b);
}

/** Why the draft cannot be added, or `null` when it can. Mirrors the core's
 * three refusals — empty, the reserved "no context" label, a duplicate under
 * the matching rule — so Add is disabled for exactly the drafts the core
 * would refuse. */
export function contextAddProblem(
  draft: string,
  existing: readonly ContextEntryDTO[],
): string | null {
  const name = draft.trim();
  if (name.length === 0) {
    return "Type a context to add.";
  }
  if (name.toLowerCase() === "no context") {
    return "That name is reserved for items with no context.";
  }
  if (existing.some((entry) => sameContext(entry.name, name))) {
    return "That context is already in the list.";
  }
  return null;
}

/** What a chip's confirm step says a removal will do — the count is the
 * core's, said before the write rather than discovered after it. */
export function removalCopy(entry: ContextEntryDTO): string {
  const { name, itemCount } = entry;
  if (itemCount === 0) {
    return `Remove ${name}`;
  }
  return `Remove ${name} — clears it from ${itemCount} ${itemCount === 1 ? "item" : "items"}`;
}

/** The last list-edit failure, worded, for THIS name — `null` when the last
 * edit succeeded or was some other chip's. `bindingWriteError`'s shape for
 * the contexts vocabulary. */
export function contextEditError(
  lastEdit: TaskContextEditResult | null,
  name: string,
): string | null {
  if (lastEdit === null || !sameContext(lastEdit.name, name) || lastEdit.kind === "ok") {
    return null;
  }
  switch (lastEdit.kind) {
    case "invalid":
      return lastEdit.error ?? "That name can't be added.";
    case "unknown":
      return "That context isn't in the list any more.";
    case "busy":
      return "The core was busy. Try again.";
    case "failed":
      return lastEdit.error ?? "That edit didn't save.";
  }
}
