// PROTOTYPE — throwaway. Delete with the rest of this directory (#120).
//
// The driveable world: you are the clock and you are the city. A pure
// `replay(commands)` rather than mutable state, so the whole scenario lives
// in the URL (`?wastepane&do=x2.n.n`) and is shareable and reloadable — the
// same trick `prototype-race-pane` plays with `?scenario`, one size up.
//
// Rewritten with `waste.ts` (2026-08-10) around one collection day and the
// operator's four states. The four are reachable directly (`preset`), because
// there is no longer any point walking a fortnight forward to see recycling;
// the day-by-day driver survives because dormant-vs-Sunday and the alert
// lifecycle are exactly what a preset cannot show.

import {
  type AlertRow,
  type Cadence,
  type CollectionReading,
  type Day,
  type Payload,
  type Stream,
  judge,
  latestOnOrBefore,
  mint,
  nextOnOrAfter,
  occurrenceKey,
} from "./waste";

/** Monday 2026-08-03. Day 0 is 1970-01-01. */
const ANCHOR_MONDAY: Day = Math.floor(Date.UTC(2026, 7, 3) / 86_400_000);
/** Start on a Wednesday, mid-week, so the first frame is the dormant state
 * the tile is in most of the time. */
const START: Day = ANCHOR_MONDAY + 9;

/** Collection is weekly; recycling rides along every other week. */
export const COLLECTION: Cadence = { anchor: ANCHOR_MONDAY, everyNWeeks: 1 };
const RECYCLING_EVERY = 2;

/** The four states the operator says exist, in the order the bar shows them. */
export const PRESETS = [
  { key: "standard", label: "standard", recycling: false, holiday: false },
  { key: "standard-recycling", label: "standard + recycling", recycling: true, holiday: false },
  { key: "holiday", label: "holiday", recycling: false, holiday: true },
  { key: "holiday-recycling", label: "holiday + recycling", recycling: true, holiday: true },
] as const;

/** The day starts at 06:00 and every action costs a minute. A real clock is
 * load-bearing: ADR-0014's live predicate compares `raisedAt` against
 * `dismissedAt`, so a re-raise stamped with the poll's *nominal schedule
 * slot* ("today at 06:00", a cron bucket) rather than the actual write time
 * lands at or before an ack made later that morning and is silently
 * swallowed. This bit while driving the terminal twin. */
const DAY_START_HOUR = 6;

export interface World {
  today: Day;
  /** Which Monday the fortnightly recycling run is anchored to. A preset
   * moves this rather than faking the reading, so the payload the pane reads
   * is always one the real cadence could have produced. */
  recyclingAnchor: Day;
  /** The city page's holiday moves, keyed by the scheduled Monday. The
   * adapter never sees this — only the date the page prints. */
  slides: Record<number, Day>;
  payload: Payload | null;
  fetchedAt: number;
  alerts: AlertRow[];
  minutes: number;
  log: string[];
}

export function now(world: World): number {
  return world.today * 86_400 + DAY_START_HOUR * 3600 + world.minutes * 60;
}

function streamsOn(world: World, collectionDay: Day): Stream[] {
  const weeksSinceAnchor = Math.round((collectionDay - world.recyclingAnchor) / 7);
  const recycles = ((weeksSinceAnchor % RECYCLING_EVERY) + RECYCLING_EVERY) % RECYCLING_EVERY === 0;
  return recycles ? ["trash", "recycling", "yard"] : ["trash", "yard"];
}

/** What the city page prints today: the upcoming collection, and whether a
 * holiday has moved it. A move that has already passed rolls forward like any
 * other completed collection. */
export function pageReading(world: World): CollectionReading {
  const current = latestOnOrBefore(COLLECTION, world.today);
  const pending = world.slides[current];
  const scheduled = pending !== undefined && pending >= world.today
    ? current
    : nextOnOrAfter(COLLECTION, world.today);
  const collectedOn = world.slides[scheduled] ?? scheduled;
  return { cadence: COLLECTION, scheduled, collectedOn, streams: streamsOn(world, scheduled) };
}

function initial(): World {
  return {
    today: START,
    recyclingAnchor: ANCHOR_MONDAY,
    slides: {},
    payload: null,
    fetchedAt: 0,
    alerts: [],
    minutes: 0,
    log: [],
  };
}

/** One daily poll: fetch, replace the snapshot wholesale, judge it, upsert
 * whatever the judgment asks for. */
function poll(world: World): void {
  const collection = pageReading(world);
  const at = now(world);
  const deviation = judge(collection);
  const key = occurrenceKey(deviation);
  const existing = key ? world.alerts.find((a) => a.sourceKey === key) : undefined;
  const up = mint(deviation, existing, collection.streams);

  let said = "snapshot replaced, nothing to say";
  if (up && existing) {
    // A value-identical upsert is no write at all. A daily poll of an
    // unchanged holiday would otherwise bump `version` every morning and push
    // a meaningless delta to every device — the no-op rule #221 landed for
    // rules PATCHes.
    const identical =
      existing.title === up.title &&
      existing.body === up.body &&
      existing.expiresAt === up.expiresAt;
    if (!identical || up.restampRaisedAt) {
      existing.title = up.title;
      existing.body = up.body;
      existing.expiresAt = up.expiresAt;
      existing.version += 1;
      if (up.restampRaisedAt) {
        existing.raisedAt = at;
        said = `re-raised: ${up.title}`;
      } else {
        said = `updated ${up.sourceKey} quietly`;
      }
    }
  } else if (up) {
    world.alerts.push({
      source: up.source,
      sourceKey: up.sourceKey,
      title: up.title,
      body: up.body,
      raisedAt: at,
      dismissedAt: null,
      expiresAt: up.expiresAt,
      version: 1,
    });
    said = `raised: ${up.title}`;
  }

  world.payload = { collection };
  world.fetchedAt = at;
  world.log.push(`poll — ${said}`);
}

export type Command =
  | { kind: "day" }
  /** Jump forward to the next occurrence of a weekday (0 = Sunday). */
  | { kind: "goto"; weekday: number }
  | { kind: "poll" }
  | { kind: "ack" }
  | { kind: "preset"; index: number };

function applyPreset(world: World, index: number): void {
  const preset = PRESETS[index] ?? PRESETS[0];
  const upcoming = nextOnOrAfter(COLLECTION, world.today);

  // Move the fortnightly anchor so the UPCOMING collection has (or has not)
  // recycling, rather than faking the payload.
  world.recyclingAnchor = preset.recycling ? upcoming : upcoming + 7;

  world.slides = {};
  if (preset.holiday) world.slides[upcoming] = upcoming + 1;

  // A preset rewrites the world the poll reads, so anything already raised is
  // about a week that no longer exists.
  world.alerts = [];
  world.log.push(`preset — ${preset.label}`);
  poll(world);
}

/** 0 = Sunday, matching `Date.getUTCDay`. Day 0 (1970-01-01) was a Thursday. */
function weekdayIndex(day: Day): number {
  return (((day + 4) % 7) + 7) % 7;
}

function step(world: World, command: Command): void {
  world.minutes += 1;
  switch (command.kind) {
    case "day":
      world.today += 1;
      world.minutes = 0;
      poll(world);
      break;
    case "goto": {
      const ahead = (command.weekday - weekdayIndex(world.today) + 7) % 7 || 7;
      world.today += ahead;
      world.minutes = 0;
      poll(world);
      break;
    }
    case "poll":
      poll(world);
      break;
    case "preset":
      applyPreset(world, command.index);
      break;
    case "ack": {
      const at = now(world);
      let n = 0;
      for (const alert of world.alerts) {
        if ((alert.dismissedAt === null || alert.raisedAt > alert.dismissedAt) && alert.expiresAt > at) {
          alert.dismissedAt = at;
          n += 1;
        }
      }
      // "Ack" is the gesture; `dismissed_at` is the column it writes.
      world.log.push(`acked ${n} live alert(s)`);
      break;
    }
  }
}

/** The whole world, rebuilt from its command history. */
export function replay(commands: Command[]): World {
  const world = initial();
  poll(world);
  for (const command of commands) step(world, command);
  world.log = world.log.slice(-4);
  return world;
}

// --- URL encoding ------------------------------------------------------

export function encode(commands: Command[]): string {
  return commands
    .map((c) => {
      switch (c.kind) {
        case "day":
          return "n";
        case "goto":
          return `g${c.weekday}`;
        case "poll":
          return "p";
        case "ack":
          return "a";
        case "preset":
          return `x${c.index}`;
      }
    })
    .join(".");
}

export function decode(raw: string | null): Command[] {
  if (!raw) return [];
  const out: Command[] = [];
  for (const token of raw.split(".")) {
    if (token === "n") out.push({ kind: "day" });
    else if (token === "p") out.push({ kind: "poll" });
    else if (token === "a") out.push({ kind: "ack" });
    else if (token.startsWith("g")) out.push({ kind: "goto", weekday: clamp(token.slice(1), 6) });
    else if (token.startsWith("x")) out.push({ kind: "preset", index: clamp(token.slice(1), PRESETS.length - 1) });
  }
  return out;
}

function clamp(raw: string, max: number): number {
  const n = Number(raw);
  return Number.isInteger(n) && n >= 0 && n <= max ? n : 0;
}
