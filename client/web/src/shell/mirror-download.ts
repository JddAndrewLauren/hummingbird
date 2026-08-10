// S9's mirror download button. `mirrorSnapshotFilename` is the one decision
// here worth a pure, tested function; actually writing bytes to disk is DOM
// glue this repo's test tooling has nowhere to run (no DOM env — see
// vitest.config.ts) and nothing to decide beyond "do the obvious thing with
// a Blob and an anchor", so it stays untested, thin wire-up.

/** A sortable, filesystem-safe name for one mirror export — colons and dots
 * in an ISO timestamp are not safe in a filename on every platform. */
export function mirrorSnapshotFilename(nowMs: number): string {
  const iso = new Date(nowMs).toISOString().replace(/[:.]/g, "-");
  return `hummingbird-mirror-${iso}.json`;
}

/** Writes `mirror` to disk as readable, pretty-printed JSON — ADR-0007's
 * "absence demotes, never deletes" is what makes the mirror worth exporting
 * this early: it is the one place that keeps history Linear itself
 * archives away. */
export function downloadMirrorSnapshot(mirror: unknown, nowMs: number): void {
  const json = JSON.stringify(mirror, null, 2);
  const blob = new Blob([json], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  try {
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = mirrorSnapshotFilename(nowMs);
    anchor.click();
  } finally {
    URL.revokeObjectURL(url);
  }
}
