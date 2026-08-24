import type { DiagnosticEventV1DTO } from "../store/protocol";

// #707's "Download diagnostics" button — the same shape as
// `mirror-download.ts`'s mirror download (that file's own doc explains why
// actually writing bytes to disk stays thin and untested wire-up: no DOM
// env in this repo's test tooling). The one thing worth its own pure,
// tested function is the filename, same as the mirror's.
//
// The export is deliberately its own file, never the mirror's: the Agent
// Brief's own acceptance criterion is that a diagnostics export "never
// carries the mirror" — two separate downloads, two separate files, is
// what makes that true by construction rather than by a shared writer
// happening to be called with the right argument.

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
 * own cumulative counter, `diagnostics-store.ts`). */
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
