// Splitting an NDJSON stream into lines, and nothing else.
//
// A `ReadableStream` chunk has no relationship to a line: one line can span
// three chunks, a chunk can end exactly on a newline, and the last line of
// a stream may arrive with no newline after it at all. This module is the
// one place that is dealt with, so `run-skill.ts` never carries a partial
// line of its own.
//
// **UTF-8 boundary safety is deliberately not here.** A multi-byte
// character split across two chunks is a *decoding* problem, and it is
// solved by `TextDecoder(…, {stream: true})` in the seam — this module only
// ever sees text that has already been decoded correctly. Putting a second
// answer to that question here would give the app two.

export interface TakenLines {
  /** Complete lines, in order, with the newline stripped. Empty lines are
   * dropped: `\n\n` in an NDJSON stream carries nothing. */
  lines: string[];
  /** Whatever followed the last newline — carry it into the next call. */
  rest: string;
}

/**
 * Fold one chunk into whatever was left over from the last one.
 *
 * A trailing `\r` is stripped, so a stream written with CRLF endings reads
 * the same as one written with LF — the runner writes LF, but a proxy or a
 * platform in between is not obliged to.
 */
export function takeLines(pending: string, chunk: string): TakenLines {
  const buffer = pending + chunk;
  const parts = buffer.split("\n");
  // The last part is by definition not newline-terminated: either a partial
  // line, or "" when the buffer ended exactly on a newline.
  const rest = parts.pop() ?? "";
  const lines = parts.map((line) => line.replace(/\r$/, "")).filter((line) => line.length > 0);
  return { lines, rest };
}
