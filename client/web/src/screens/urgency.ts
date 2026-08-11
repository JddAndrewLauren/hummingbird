// CONTEXT.md: "Urgency … is computed by consumers at read time over the
// mirror. Never a stored class and never a routing decision at ingestion."
// This module is that read-time computation for the web host, over the raw
// `deadline` string ADR-0009/0013 defines (`YYYY-MM-DD` or
// `YYYY-MM-DDTHH:MM`, naive local, minute precision — never a UTC instant,
// so this parses it as *local* wall-clock time, matching the ADR's own "a
// calendar date is not an instant" reasoning). Called fresh on every render
// from `ItemRow`'s `urgency` prop; nothing here is ever written back to a
// `TaskItemDTO` or the store — see `store/protocol.ts`'s `TaskItemDTO`,
// which carries no urgency field at all.

export type Urgency = "calm" | "soon" | "now" | "overdue";

/** ADR-0013's comparison key: a day-only deadline means "by the end of that
 * day", so it resolves to `T23:59` before any comparison — the TS twin of
 * `hummingbird_domain::deadline_sort_key` (`server/domain/src/deadline.rs`).
 * A minute-precision value is returned unchanged. */
export function deadlineSortKey(deadline: string): string {
  return deadline.length === 10 ? `${deadline}T23:59` : deadline;
}

/** The TS twin of `hummingbird_domain::is_valid_deadline`
 * (`server/domain/src/deadline.rs`) — `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM` and
 * nothing else: no seconds, no `Z`/offset, no bare time, and no calendar date
 * that does not exist (`2026-02-30` is refused, and leap years are real).
 *
 * A twin rather than a second opinion: the seam
 * (`client/ffi-web/src/task_host.rs`) checks the same rule with the domain
 * function itself and refuses a bad value there, so this exists only so a form
 * can say *which field* is wrong while someone is still typing, instead of the
 * edit failing after the fact. `Date` does the calendar arithmetic, which is
 * how leap years stay correct here without restating the rule. */
export function isValidDeadline(deadline: string): boolean {
  const dateOnly = /^(\d{4})-(\d{2})-(\d{2})$/.exec(deadline);
  const dateTime = DEADLINE_PATTERN.exec(deadline);
  const match = dateOnly ?? dateTime;
  if (!match) {
    return false;
  }
  const [, year, month, day] = match;
  const parsed = new Date(Number(year), Number(month) - 1, Number(day));
  // Round-tripping is the existence check: `new Date(2026, 1, 30)` silently
  // rolls over to March 2nd, so a date that does not exist comes back with a
  // different month or day than it went in with.
  if (
    parsed.getFullYear() !== Number(year) ||
    parsed.getMonth() !== Number(month) - 1 ||
    parsed.getDate() !== Number(day)
  ) {
    return false;
  }
  if (dateTime) {
    const [, , , , hour, minute] = dateTime;
    return Number(hour) <= 23 && Number(minute) <= 59;
  }
  return true;
}

/** A scheduled date is a whole civil day — a do-date has no minute — so the
 * date-time form `isValidDeadline` also accepts is refused here. */
export function isValidScheduledDate(scheduledDate: string): boolean {
  return scheduledDate.length === 10 && isValidDeadline(scheduledDate);
}

const DEADLINE_PATTERN = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/;

/** Parses a resolved deadline key as local wall-clock time. `null` for
 * anything that does not match the shape (defensive — a caller is expected
 * to already hold an `is_valid_deadline`-accepted string, but this must
 * never throw on one that somehow is not). */
function deadlineToMs(deadline: string): number | null {
  const match = DEADLINE_PATTERN.exec(deadlineSortKey(deadline));
  if (!match) {
    return null;
  }
  const [, year, month, day, hour, minute] = match;
  const date = new Date(
    Number(year),
    Number(month) - 1,
    Number(day),
    Number(hour),
    Number(minute),
  );
  return Number.isNaN(date.getTime()) ? null : date.getTime();
}

/** Past due within this window reads as "now" rather than merely "soon" —
 * generous enough that a same-day deadline is never mistaken for something
 * days off, tight enough that "soon" still means "not today". */
const NOW_WINDOW_MS = 24 * 60 * 60 * 1000;

/** Beyond `NOW_WINDOW_MS` but inside this window reads as "soon"; beyond it,
 * "calm". Three days: long enough to surface a coming deadline without
 * making most of a normal backlog read as urgent. */
const SOON_WINDOW_MS = 3 * 24 * 60 * 60 * 1000;

/** The one urgency computation this app has: no deadline, or one this
 * module cannot parse, is "calm" — never an error and never treated as
 * "no consequence" the other direction (overdue). */
export function computeUrgency(deadline: string | null, nowMs: number): Urgency {
  if (deadline === null) {
    return "calm";
  }
  const deadlineMs = deadlineToMs(deadline);
  if (deadlineMs === null) {
    return "calm";
  }
  const remainingMs = deadlineMs - nowMs;
  if (remainingMs < 0) {
    return "overdue";
  }
  if (remainingMs <= NOW_WINDOW_MS) {
    return "now";
  }
  if (remainingMs <= SOON_WINDOW_MS) {
    return "soon";
  }
  return "calm";
}
