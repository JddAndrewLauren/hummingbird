// The `within_next`/`within_last` value shape (ADR-0013): a bare integer
// plus a unit suffix — `m`/`h`/`d` for a `timestamp` field, `d` only for a
// `date` field (ADR-0013's own table). This module only parses/formats and
// measures against the DO alarm interval (#138) for the duration-warning
// acceptance criterion; it never decides legality — `operators.ts` does.

export type DurationUnit = "m" | "h" | "d";

const UNIT_MS: Record<DurationUnit, number> = {
  m: 60_000,
  h: 60 * 60_000,
  d: 24 * 60 * 60_000,
};

/** Parses a wire duration string (`"2h"`, `"10m"`, `"3d"`) into its
 * milliseconds — the same value ADR-0013's engine measures a condition
 * against. `undefined` for anything that is not a bare positive integer
 * plus one of `m`/`h`/`d`. */
export function parseDurationMs(value: string): number | undefined {
  const match = /^(\d+)(m|h|d)$/.exec(value.trim());
  if (match === null) {
    return undefined;
  }
  const amount = Number(match[1]);
  const unit = match[2] as DurationUnit;
  if (amount <= 0) {
    return undefined;
  }
  return amount * UNIT_MS[unit];
}

/** The wire string for `amount` of `unit` — the inverse of
 * [`parseDurationMs`], for a duration picker to write back. */
export function formatDuration(amount: number, unit: DurationUnit): string {
  return `${amount}${unit}`;
}

/** The units a duration picker offers for one field type — ADR-0013's own
 * table: a `date` field is day-grained only, since a sub-day offset
 * against a day-only value is meaningless; a `timestamp` field gets all
 * three. */
export function durationUnitsFor(fieldType: "timestamp" | "date"): DurationUnit[] {
  return fieldType === "date" ? ["d"] : ["m", "h", "d"];
}

/** **Warn — never reject** — when a duration is shorter than the DO alarm
 * interval (#138): a rule that fires less precisely than the operator
 * intended is still legitimate, so this is read-only decision material for
 * a warning banner, never something `canSubmitRule`-style gating uses to
 * block a save. `undefined`/unparseable input warns nothing — a malformed
 * duration is #133's save-time rejection to catch, not this one's. */
export function isBelowAlarmInterval(value: string, alarmIntervalMs: number): boolean {
  const ms = parseDurationMs(value);
  return ms !== undefined && ms < alarmIntervalMs;
}
