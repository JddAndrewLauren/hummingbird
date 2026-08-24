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

/** The exported document's own shape — `dropped_count` travels alongside
 * `events` so a reader can tell "a quiet 72 hours" from "a quiet 72 hours,
 * and 4,000 events this journal could not afford to keep" (the journal's
 * own cumulative counter, `diagnostics-store.ts`).
 *
 * **#712 reconciliation of the divergence #707/#709 left standing (review
 * round 2 of PR #736).** This envelope used to be `{"events",
 * "droppedCount"}` — camelCase, no envelope-level `schema_version` — while
 * Android's (#709) was already `{"schema_version", "dropped_count",
 * "events"}`. `protocol.ts` states a cross-host boundary keeps snake_case,
 * and this envelope is exactly such a boundary (an operator diffs a phone
 * export against a browser export), so this side is the one that moved:
 * it now matches Android's key set, casing and `schema_version` value
 * exactly. The per-event records were never divergent — both hosts already
 * write the identical snake_case `DiagnosticEventV1` (see
 * `DiagnosticEventV1DTO`'s own doc in `store/protocol.ts`) — so only the
 * three envelope keys changed. `1` is `DIAGNOSTIC_EVENT_SCHEMA_VERSION`'s
 * value (`server/domain/src/diagnostics.rs`); there is no shared TS
 * constant to import across the wasm boundary for an envelope literal, so
 * it is hand-written here the same way Android hand-writes `1` in
 * `DiagnosticJournal.kt`/`DiagnosticsRecorder.kt` — see `docs/diagnostics.md`
 * for the fuller picture. */
export interface DiagnosticsExportDocument {
  schema_version: number;
  dropped_count: number;
  events: DiagnosticEventV1DTO[];
}

/** Writes the journal export to disk as readable, pretty-printed JSON —
 * `downloadMirrorSnapshot`'s own Blob-and-anchor idiom, unchanged. */
export function downloadDiagnosticsExport(
  events: DiagnosticEventV1DTO[],
  droppedCount: number,
  nowMs: number,
): void {
  const exportDocument: DiagnosticsExportDocument = {
    schema_version: 1,
    dropped_count: droppedCount,
    events,
  };
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
