import type { Band } from "../screens/questions/contract";
import { bandTone } from "../screens/status-board/tile-copy";

// The Status nav control's tint, and the one thing this layer is allowed to
// decide about it.
//
// **Which band came back is the core's answer** (`decisions/panes/alarm.rs`,
// reached through `statusAlarm`), not this file's: the two opinions that
// fold makes — a gap is silent, `dormant` is silent, everything else is not
// — are the same on the phone, the desktop and Android, so they live once,
// in Rust. What is left here is the last step alone, the step ADR-0025
// leaves per-client: which colour a band paints as.
//
// **And even that is not a new mapping.** `bandTone` is the board's own
// band→treatment table, imported rather than restated, so the button and the
// tiles it expands into can never disagree about how bad the same answer is.
// The direction of the import (shell reading from a screen) is the price of
// having exactly one table; the alternative was a second copy in `shell/`,
// which is the thing that would actually rot.

/** The colour the Status control wears for `band`, or `undefined` when it
 * should wear its ordinary nav colour.
 *
 * `undefined` in, `undefined` out — "nothing raises the nav" and "this band
 * is quiet" are the same instruction to a caller, and splitting them would
 * make every call site handle two spellings of one answer. */
export function navAlarmColor(band: Band | undefined): string | undefined {
  if (band === undefined) return undefined;
  switch (bandTone(band)) {
    case "danger":
      return "var(--status-danger-fg)";
    case "warn":
      return "var(--status-warn-fg)";
    // `dormant` reaches here only if the core ever starts returning it; the
    // nav wears its ordinary colour rather than inventing a third tint.
    default:
      return undefined;
  }
}
