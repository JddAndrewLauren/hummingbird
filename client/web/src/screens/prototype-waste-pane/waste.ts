// PROTOTYPE — throwaway. Delete with the rest of this directory (#120).
//
// The read-time logic for the which-cans pane, rewritten 2026-08-10 against
// the operator's account of how the real service actually works:
//
//   * there is only ever ONE collection day — every stream that goes out that
//     week goes out together, so a per-stream "next date" was modelling a
//     variation that does not exist;
//   * there are only four states — standard or holiday, times with or without
//     recycling. Trash and yard go every week, recycling every other;
//   * the tile is dormant most of the week, wakes on Sunday (the night the
//     cans go out), and shouts only when a holiday moves the day.
//
// That collapse is why `Deviation` no longer has a skipped-cycle arm: a week
// with no collection at all is not one of the four states.
//
// Dates are whole days since the Unix epoch — an integer, never a `Date` — so
// a collection day cannot drift across a timezone the way a UTC instant does.
//
// NOTE: the Rust twin in `server/prototypes/waste-cadence/` still models the
// superseded per-stream world; its lifecycle findings (restamp rules, expiry,
// no-op writes) all survive this change, its fan-out does not.

/** Both the snapshot's `source` and every alert's `source`: ADR-0009's join
 * constraint is that these are one string, so there is one constant. `/v2`
 * because `city-waste/v1` is retired in the frozen registry. */
export const SOURCE = "city-waste/v2";
export const SNAPSHOT_KEY = "collection";

export type Day = number;

export type Stream = "trash" | "recycling" | "yard";

/** The bins' own colours, as they are on the kerb: grey, light blue, green.
 *
 * A deliberate departure from the design system's "status colour always
 * encodes stage, tier or urgency, never decoration" rule — these encode
 * *object identity*, which is the one thing on this tile a person matches
 * against the real world before walking outside. They are literal values
 * rather than brand tokens for the same reason: the bins are not part of the
 * palette. `fill` is the body, `edge` the lid and outline; both are picked to
 * hold up on the sand and the ink surfaces without a per-theme variant. */
export const BIN: Record<Stream, { fill: string; edge: string; label: string }> = {
  trash: { fill: "#9aa3ab73", edge: "#79838b", label: "trash" },
  recycling: { fill: "#7fc4e873", edge: "#3f93c4", label: "recycling" },
  yard: { fill: "#6aa84f73", edge: "#4d8a3a", label: "yard" },
};

export const STREAM_ORDER: Stream[] = ["trash", "recycling", "yard"];

const WEEKDAYS = ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"];

export function weekday(day: Day): string {
  return WEEKDAYS[((day % 7) + 7) % 7];
}

export function iso(day: Day): string {
  return new Date(day * 86_400_000).toISOString().slice(0, 10);
}

export function shortDate(day: Day): string {
  return `${weekday(day).slice(0, 3)} ${iso(day)}`;
}

/** Local midnight after this day, in epoch seconds — ADR-0014's "end of the
 * affected collection date". */
export function endOfDay(day: Day): number {
  return (day + 1) * 86_400;
}

/** The collection rhythm: an anchor Monday and a period. One cadence for the
 * whole service now, not one per stream. */
export interface Cadence {
  anchor: Day;
  everyNWeeks: number;
}

export function periodDays(cadence: Cadence): number {
  return 7 * cadence.everyNWeeks;
}

export function nextOnOrAfter(cadence: Cadence, day: Day): Day {
  const period = periodDays(cadence);
  const delta = day - cadence.anchor;
  const whole = Math.floor(delta / period);
  return cadence.anchor + (delta - whole * period === 0 ? whole : whole + 1) * period;
}

export function latestOnOrBefore(cadence: Cadence, day: Day): Day {
  const period = periodDays(cadence);
  return cadence.anchor + Math.floor((day - cadence.anchor) / period) * period;
}

/** One collection as the page reports it: the day, and which cans go out on
 * it. `collectedOn` differs from `scheduled` only on a holiday week. */
export interface CollectionReading {
  cadence: Cadence;
  scheduled: Day;
  collectedOn: Day;
  streams: Stream[];
}

/** The whole `context_snapshots.payload`, replaced wholesale each poll. */
export interface Payload {
  collection: CollectionReading;
}

export function payloadJson(payload: Payload): string {
  const c = payload.collection;
  return JSON.stringify({
    cadence: { anchor: iso(c.cadence.anchor), every_n_weeks: c.cadence.everyNWeeks },
    collection_date: iso(c.collectedOn),
    streams: c.streams,
  });
}

export type Deviation =
  /** The page's collection day is exactly where the cadence puts it —
   * including the roll-forward, which is a big diff and no deviation at all. */
  | { kind: "on-cadence" }
  /** A holiday moved this week's collection later. `scheduled` is the fixed
   * coordinate the alert's key is built from. */
  | { kind: "holiday"; scheduled: Day; slidesTo: Day };

/** Materiality, judged from the reading alone — deliberately never from the
 * previous snapshot. That is why the roll-forward is silent: the morning
 * after a collection the page jumps a whole week, which is a large diff and a
 * zero deviation, and it is also why the first poll after a wiped snapshot is
 * still correct. */
export function judge(reading: CollectionReading): Deviation {
  const scheduled = latestOnOrBefore(reading.cadence, reading.collectedOn);
  if (reading.collectedOn === scheduled) return { kind: "on-cadence" };
  return { kind: "holiday", scheduled, slidesTo: reading.collectedOn };
}

/** ADR-0014's v1 recipe, carried forward and simplified with the domain: the
 * originally scheduled collection date, so a correction to the slid-to date
 * lands on the same row rather than minting a second alert about one
 * holiday. */
export function occurrenceKey(deviation: Deviation): string | null {
  return deviation.kind === "on-cadence" ? null : `collection:${iso(deviation.scheduled)}`;
}

/** Exactly the `alerts` columns this prototype touches. */
export interface AlertRow {
  source: string;
  sourceKey: string;
  title: string;
  body: string;
  raisedAt: number;
  dismissedAt: number | null;
  expiresAt: number;
  version: number;
}

/** ADR-0014's live predicate — the twin of `hummingbird_domain::is_live`. A
 * lifecycle stamp is current only if the alert has not been raised since. */
export function isLive(row: AlertRow, now: number): boolean {
  const notDismissed = row.dismissedAt === null || row.raisedAt > row.dismissedAt;
  return notDismissed && row.expiresAt > now;
}

export interface AlertUpsert {
  source: string;
  sourceKey: string;
  title: string;
  body: string;
  /** True only when the row is new *or* the change itself changed. False on
   * every daily re-poll in between — which is what keeps an ack acked. */
  restampRaisedAt: boolean;
  /** End of the *later* of scheduled and slid-to. Expiring at the end of the
   * originally scheduled Monday would take the holiday text off the pane on
   * the Tuesday morning it exists to warn about. */
  expiresAt: number;
}

export function mint(
  deviation: Deviation,
  existing: AlertRow | undefined,
  streams: Stream[],
): AlertUpsert | null {
  if (deviation.kind === "on-cadence") return null;

  // The alert states the change itself, never that one happened.
  const title = `Collection: ${weekday(deviation.scheduled)} → ${weekday(deviation.slidesTo)} this week`;
  const listed = sentenceList(streams);
  const body = `${listed[0].toUpperCase()}${listed.slice(1)} normally collected ${weekday(deviation.scheduled)}. This week the pickup is ${weekday(deviation.slidesTo)} ${iso(deviation.slidesTo)}.`;

  return {
    source: SOURCE,
    sourceKey: `collection:${iso(deviation.scheduled)}`,
    title,
    body,
    // A change to the change (the city correcting Tue → Wed) is a new thing
    // to say, so it earns a fresh raise even over an ack. Anything else — the
    // same holiday re-read on each of the next few mornings — must not
    // restamp, or the ack is undone daily.
    restampRaisedAt: existing === undefined || existing.title !== title || existing.body !== body,
    expiresAt: endOfDay(Math.max(deviation.scheduled, deviation.slidesTo)),
  };
}

export function sentenceList(streams: Stream[]): string {
  const names = streams.map((s) => BIN[s].label);
  if (names.length <= 1) return names[0] ?? "nothing";
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

// --- the pane ----------------------------------------------------------

/** How loudly the tile presents itself. The tile is furniture most of the
 * week, and since there is no longer a separate holiday AlertCard, it is also
 * the *only* signal a holiday ever gets — so a holiday week is prominent for
 * its whole length, not just on the eve.
 *
 * - `dormant`   — one quiet line; it must not compete with the frontier.
 * - `prominent` — the eve (Sunday), the day itself (Monday), or any holiday.
 */
export type Prominence = "dormant" | "prominent";

export interface PaneView {
  streams: Stream[];
  collectionDate: Day;
  daysAway: number;
  /** True on the collection day itself. */
  today: boolean;
  /** The city moved this week's collection off its cadence day. Read straight
   * off the snapshot — NOT off a live alert. The pane no longer joins to the
   * alert lane, because with no card to ack there is nothing to quiet: the
   * changed day IS the answer, not an interruption laid over it. */
  holiday: boolean;
  prominence: Prominence;
  headline: string;
  staleMinutes: number;
}

/** The whole sentence, and deliberately a short one — the coloured bins say
 * which cans, so the words only have to say when. */
export function headline(collectionDate: Day, today: Day, holiday: boolean): string {
  if (collectionDate === today) return "Trash Today";
  // A holiday names its day even when that day is tomorrow: on the one week
  // the day is unusual, "Tonight" would hide the very thing that changed.
  if (holiday) return `Trash ${weekday(collectionDate)}`;
  if (collectionDate - today === 1) return "Trash Tonight";
  return `Trash ${weekday(collectionDate)}`;
}

/** The standing question, answered over the snapshot. Computed fresh at read
 * time; nothing here is ever written back. */
export function pane(
  payload: Payload,
  today: Day,
  now: number,
  fetchedAt: number,
): PaneView {
  const collection = payload.collection;
  const holiday = collection.collectedOn !== collection.scheduled;
  const daysAway = collection.collectedOn - today;

  return {
    streams: collection.streams,
    collectionDate: collection.collectedOn,
    daysAway,
    today: daysAway === 0,
    holiday,
    prominence: holiday || daysAway <= 1 ? "prominent" : "dormant",
    headline: headline(collection.collectedOn, today, holiday),
    staleMinutes: Math.floor((now - fetchedAt) / 60),
  };
}
