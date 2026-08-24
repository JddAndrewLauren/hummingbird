import type { DiagnosticEventV1DTO } from "../store/protocol";

// #707's "Download diagnostics" button — the same Blob-and-anchor shape as
// `mirror-download.ts`'s mirror download.
//
// **Review round 1 of PR #736 caught a false comment here**: this used to
// say "no DOM env in this repo's test tooling", which is not true — this
// repo's `// @vitest-environment jsdom` docblock (used by every
// `*.test.tsx` component test) is exactly how `diagnostics-download.test.ts`
// exercises this file's actual Blob content, not just its filename.
//
// The export is deliberately its own file, never the mirror's: the Agent
// Brief's own acceptance criterion is that a diagnostics export "never
// carries the mirror" — two separate downloads, two separate files, is
// what makes that true by construction rather than by a shared writer
// happening to be called with the right argument. `diagnostics-download.test.ts`
// proves it by reading the actual Blob's text back and asserting the word
// "mirror" never appears in it.

/** A sortable, filesystem-safe name for one diagnostics export — same
 * colon/dot substitution `mirrorSnapshotFilename` uses, for the same
 * cross-platform reason. */
export function diagnosticsExportFilename(nowMs: number): string {
  const iso = new Date(nowMs).toISOString().replace(/[:.]/g, "-");
  return `hummingbird-diagnostics-${iso}.json`;
}

/** The exported document's own shape — `droppedCount` travels alongside
 * `events` so a reader can tell "a quiet 72 hours" from "a quiet 72 hours,
 * and 4,000 events this journal could not afford to keep" (the journal's
 * own cumulative counter, `diagnostics-store.ts`).
 *
 * **KNOWN DIVERGENCE from Android's export (#709), left standing for #712
 * to reconcile — review round 2 of PR #736.** This ENVELOPE is
 * `{"events", "droppedCount"}`: camelCase, and with no envelope-level
 * `schema_version` of its own. Android's (#709, merged) is
 * `{"schema_version", "dropped_count", "events"}`. The per-event records
 * inside are *not* divergent — both hosts write the identical snake_case
 * `DiagnosticEventV1` (see `DiagnosticEventV1DTO`'s own doc in
 * `store/protocol.ts` for why that one type keeps the wire's spelling) —
 * so the divergence is exactly the two wrapper keys plus the missing
 * version field.
 *
 * That is a real inconsistency with `protocol.ts`'s own stated rule that a
 * cross-host boundary keeps snake_case, and this envelope is such a
 * boundary: an operator diffing a phone export against a browser export
 * hits it immediately. It is deliberately NOT fixed here — #712 owns
 * reconciling the two exports, and changing one side unilaterally mid-batch
 * would just move the mismatch. Whoever picks up #712: the fix is on this
 * side (snake_case the two keys, add an envelope `schema_version`), not on
 * Android's, and `diagnostics-download.test.ts` plus
 * `SettingsScreen.test.tsx`'s export-content assertions are what pin the
 * current spelling. */
export interface DiagnosticsExportDocument {
  events: DiagnosticEventV1DTO[];
  droppedCount: number;
}

/** Writes the journal export to disk as readable, pretty-printed JSON —
 * `downloadMirrorSnapshot`'s own Blob-and-anchor idiom, unchanged. */
export function downloadDiagnosticsExport(
  events: DiagnosticEventV1DTO[],
  droppedCount: number,
  nowMs: number,
): void {
  const exportDocument: DiagnosticsExportDocument = { events, droppedCount };
  const json = JSON.stringify(exportDocument, null, 2);
  const blob = new Blob([json], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  try {
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = diagnosticsExportFilename(nowMs);
    anchor.click();
  } finally {
    URL.revokeObjectURL(url);
  }
}
