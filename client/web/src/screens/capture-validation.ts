// Issue #110/S12 acceptance: "An empty capture is refused client-side — a
// junk row must never be able to wedge the queue." A whitespace-only draft
// counts as empty: `Core::capture` (client/core) has no opinion of its own
// on this — it enqueues whatever `title` it is handed — so the refusal has
// to happen here, before a request is ever sent to the worker.

/** Whether `draft` is a real capture worth submitting. Pure — no trimming
 * side effect on the caller's own state; `TriageScreen` still sends the
 * original (untrimmed) string on submit, since #110's "raw string reaches
 * the mutation unmodified" criterion means this function decides whether to
 * submit, never what gets submitted. */
export function canSubmitCapture(draft: string): boolean {
  return draft.trim().length > 0;
}
