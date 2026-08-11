import { parseDurationMs } from "./duration";

// The `deadline` date/time picker (#140, acceptance criterion 3): "picking
// a target date is the more natural gesture for a deadline specifically,
// and the picker computes the equivalent duration string underneath, so
// the wire value is still an ordinary duration" (`condition-editor.ts`'s
// own doc for `widgetFor`'s `"datetime"` branch). This module is that
// computation, pure and pickable apart from the `<input type="datetime-
// local">` it drives — a target datetime one way, the wire's duration
// string the other, both resolved against a caller-supplied `nowMs`
// (`Date.now()` at the moment of the edit, the same ephemeral read-time
// clock `BacktestPanel` already uses in this screen; there is no stored
// value here to keep clock-independent the way `alert::plan` must).
//
// Sign convention matches `backtest.ts`'s own: `within_next` fires when
// `target <= now + D` (a future or already-past deadline), `within_last`
// when `target >= now - D` (a past deadline within the window). Picking a
// target in the "wrong" direction (a past date for `within_next`, a future
// date for `within_last`) still produces a well-formed duration — clamped
// up to one minute, since `parseDurationMs` rejects a non-positive amount —
// rather than a value the picker silently refuses to write.

export type DeadlineOperator = "within_next" | "within_last";

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

/** Formats `ms` as a local `datetime-local` input value
 * (`YYYY-MM-DDTHH:mm`) — read against the device's own clock and time
 * zone, exactly like every other local-wall-clock reading this repo's
 * screens do (`urgency.ts`, `deadlineSortKey`). */
function toLocalInputValue(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** Parses a `datetime-local` input value back to epoch ms, local time.
 * `undefined` for anything the input control did not itself produce (an
 * empty string mid-edit, most commonly). */
function fromLocalInputValue(value: string): number | undefined {
  if (value.trim() === "") {
    return undefined;
  }
  const ms = new Date(value).getTime();
  return Number.isNaN(ms) ? undefined : ms;
}

/** The `datetime-local` input value that displays `durationValue` (the
 * wire's `"2h"`/`"3d"` string) as a concrete moment, given `op` and the
 * clock `nowMs`. Empty when `durationValue` does not parse — the picker
 * then starts blank rather than guessing a moment nobody chose. */
export function datetimeInputValueFromDuration(
  durationValue: string,
  op: DeadlineOperator,
  nowMs: number,
): string {
  const ms = parseDurationMs(durationValue);
  if (ms === undefined) {
    return "";
  }
  const target = op === "within_next" ? nowMs + ms : nowMs - ms;
  return toLocalInputValue(target);
}

/** The wire duration string for a `datetime-local` input's chosen moment,
 * given `op` and the clock `nowMs`. Always expressed in whole minutes —
 * exact, and `durationUnitsFor("timestamp")` (`deadline`'s own field type)
 * already permits a bare `m` suffix with no upper bound. `undefined` when
 * `inputValue` is not (yet) a complete moment, so a mid-edit keystroke
 * never writes a bogus condition value. */
export function durationFromDatetimeInputValue(
  inputValue: string,
  op: DeadlineOperator,
  nowMs: number,
): string | undefined {
  const target = fromLocalInputValue(inputValue);
  if (target === undefined) {
    return undefined;
  }
  const diffMs = op === "within_next" ? target - nowMs : nowMs - target;
  const minutes = Math.max(1, Math.round(diffMs / 60_000));
  return `${minutes}m`;
}
