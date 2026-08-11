// Civil days in somebody else's time zone (#245/#120).
//
// **Why this exists at all, stated once.** A bin collection happens on a day
// at an address, not at an instant on a device. A person reading this pane in
// a hotel three time zones away must still be told the day their own street
// is collected on, and "tonight" must flip at the address's midnight, not the
// device's. So the `city-waste/v2` payload body carries an IANA `zone`
// alongside its whole-day dates, and every day-shaped question here is
// resolved in that zone.
//
// This is a **per-pane exception**, documented at its point of use rather
// than generalised: nothing else in this app has an "somewhere else's civil
// day" problem yet, and ADR-0015's Time section says the clock crosses the
// seam as a number of milliseconds. `Intl.DateTimeFormat` is the only zone
// database available on either side of that seam without shipping one, and it
// is a browser API, so this is TS and stays TS.
//
// Pure and clock-free: every function takes the instant it works from.

/** A whole civil day, `YYYY-MM-DD` — never a `Date` and never a UTC instant,
 * both of which drift a day at a zone boundary. */
export type CivilDate = string;

const CIVIL_DATE = /^\d{4}-\d{2}-\d{2}$/;

export function isCivilDate(value: unknown): value is CivilDate {
  return typeof value === "string" && CIVIL_DATE.test(value);
}

/** Whether `zone` is an IANA zone this runtime knows. A zone it does not is
 * the payload's problem, not a crash: every caller here treats it as a
 * malformed body, which the pane renders as a gap. */
export function isKnownZone(zone: string): boolean {
  try {
    new Intl.DateTimeFormat("en-CA", { timeZone: zone });
    return true;
  } catch {
    return false;
  }
}

/** The civil date it is *right now* at the address — the day "today" means
 * to the person whose bins these are.
 *
 * `en-CA` because its short date format is already `YYYY-MM-DD`; the locale
 * is a formatting choice, not a language one, and nothing user-facing is
 * rendered from it. Returns `null` for a zone this runtime cannot resolve. */
export function civilTodayInZone(nowMs: number, zone: string): CivilDate | null {
  if (!isKnownZone(zone)) {
    return null;
  }
  const formatted = new Intl.DateTimeFormat("en-CA", {
    timeZone: zone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date(nowMs));
  return isCivilDate(formatted) ? formatted : null;
}

/** What `zone`'s clock read at the instant `utcMs`, expressed as if it were
 * UTC — the building block for turning a civil date back into an instant. */
function asIfUtcMs(utcMs: number, zone: string): number {
  const parts = new Intl.DateTimeFormat("en-CA", {
    timeZone: zone,
    hourCycle: "h23",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).formatToParts(new Date(utcMs));
  const read = (type: Intl.DateTimeFormatPartTypes): number =>
    Number(parts.find((part) => part.type === type)?.value ?? "0");
  return Date.UTC(
    read("year"),
    read("month") - 1,
    read("day"),
    read("hour"),
    read("minute"),
    read("second"),
  );
}

/** The instant `date` begins at the address — midnight, in `zone`.
 *
 * Solved by iteration rather than by an offset table: guess that the zone is
 * UTC, measure how wrong that guess was *at the guessed instant*, correct,
 * and measure once more. The second pass is what makes a DST boundary come
 * out right — the offset an hour before a transition is not the offset after
 * it, so a single correction can land on the wrong side of the change.
 * `null` for an unknown zone or a malformed date. */
export function zonedMidnightMs(date: CivilDate, zone: string): number | null {
  if (!isCivilDate(date) || !isKnownZone(zone)) {
    return null;
  }
  const naive = Date.parse(`${date}T00:00:00Z`);
  if (Number.isNaN(naive)) {
    return null;
  }
  let instant = naive - (asIfUtcMs(naive, zone) - naive);
  instant = naive - (asIfUtcMs(instant, zone) - instant);
  return instant;
}

/** Whole civil days from `from` to `to` — plain calendar arithmetic over two
 * `YYYY-MM-DD` strings, deliberately *not* a subtraction of two instants: the
 * gap between two midnights is 23 or 25 hours across a DST transition, and
 * "three days away" must stay three days through one. */
export function civilDaysBetween(from: CivilDate, to: CivilDate): number | null {
  if (!isCivilDate(from) || !isCivilDate(to)) {
    return null;
  }
  const a = Date.parse(`${from}T00:00:00Z`);
  const b = Date.parse(`${to}T00:00:00Z`);
  if (Number.isNaN(a) || Number.isNaN(b)) {
    return null;
  }
  return Math.round((b - a) / 86_400_000);
}

/** The day's name at the address — "Monday". `en-US` for the same
 * formatting-not-language reason as above; this one *is* rendered, and the
 * app is English-only (there is no i18n layer to route it through yet). */
export function weekdayInZone(date: CivilDate, zone: string): string | null {
  const midnight = zonedMidnightMs(date, zone);
  if (midnight === null) {
    return null;
  }
  return new Intl.DateTimeFormat("en-US", { timeZone: zone, weekday: "long" }).format(
    new Date(midnight),
  );
}
