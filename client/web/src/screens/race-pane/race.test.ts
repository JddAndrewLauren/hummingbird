import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import type { BindingDTO, PaneAlertDTO, PaneSnapshotDTO } from "../../store/protocol";
import { EMPTY_QUESTION_SYNC, type QuestionInputs } from "../questions/contract";
import {
  BINDING_KEY,
  SETUP_SUBJECT,
  SOURCE,
  abbreviate,
  clock,
  countdown,
  dayLabel,
  nextRaceAt,
  parseRaceBody,
  raceAnswer,
  raceHeadlineParts,
  raceSetup,
  raceSubjects,
  raceView,
  seriesFromBinding,
  type RaceEvent,
} from "./race";

// **The parser is tested against the committed contract, not a hand-typed
// copy of it.** `server/race-poll/tests/fixtures/golden-body.json` is the
// exact envelope `race-schedule-poll` emits from the committed Jolpica
// response, byte-guarded on the Rust side by `tests/golden.rs`; reading the
// real file here is this side's counterweight, the same move
// `runner/test/parse-capture.test.js` makes with the real shipped schema.
// A fixture retyped in this file would agree with itself forever while the
// two sides drifted apart.

const GOLDEN: unknown = JSON.parse(
  readFileSync(
    new URL("../../../../../server/race-poll/tests/fixtures/golden-body.json", import.meta.url),
    "utf8",
  ),
);

function goldenEnvelope(): { schema: string; polled_every_ms: number; body: unknown } {
  return GOLDEN as { schema: string; polled_every_ms: number; body: unknown };
}

export function snapshotOf(body: unknown, overrides: Partial<PaneSnapshotDTO> = {}): PaneSnapshotDTO {
  return {
    key: "f1",
    fetchedAtMs: 0,
    envelope: {
      kind: "ok",
      schema: SOURCE,
      polledEveryMs: 6 * 60 * 60 * 1000,
      body: JSON.stringify(body),
    },
    freshness: { kind: "age", ageMs: 60_000, declaredCadenceMs: 6 * 60 * 60 * 1000 },
    ...overrides,
  };
}

describe("parseRaceBody", () => {
  it("reads the season out of the poller's own committed golden body", () => {
    const parsed = parseRaceBody(snapshotOf(goldenEnvelope().body));
    expect(parsed.kind).toBe("ok");
    if (parsed.kind !== "ok") {
      return;
    }
    const first = parsed.body.events[0];
    expect(first.name).toBe("Australian Grand Prix");
    expect(first.locality).toBe("Melbourne");
    expect(first.startsAtMs).toBe(1_772_942_400_000);
    expect(first.sessions[0]).toEqual({
      kind: "practice",
      label: "Practice 1",
      startsAtMs: 1_772_760_600_000,
    });
    // The shape rule the golden file states on its own bytes, read back
    // through this parser: `sessions` is the supporting ladder and never
    // holds the race, whose start is the event's own.
    expect(Math.max(...first.sessions.map((session) => session.startsAtMs))).toBeLessThan(
      first.startsAtMs,
    );
  });

  it("names each way a payload can fail to be a season, and never answers a quiet empty one", () => {
    expect(parseRaceBody(undefined)).toEqual({
      kind: "gap",
      reason: expect.stringContaining("fetched"),
    });
    expect(
      parseRaceBody(snapshotOf(null, { envelope: { kind: "malformed", reason: "no `schema`" } })),
    ).toEqual({ kind: "gap", reason: expect.stringContaining("no `schema`") });
    expect(
      parseRaceBody(
        snapshotOf(null, {
          envelope: { kind: "ok", schema: "race-schedule/v2", polledEveryMs: null, body: "{}" },
        }),
      ),
    ).toEqual({ kind: "gap", reason: expect.stringContaining("race-schedule/v2") });
    expect(
      parseRaceBody(
        snapshotOf(null, {
          envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: "{" },
        }),
      ).kind,
    ).toBe("gap");
    expect(parseRaceBody(snapshotOf({ events: "the whole season, honest" })).kind).toBe("gap");
    expect(parseRaceBody(snapshotOf({ events: [{ name: "A GP" }] })).kind).toBe("gap");
  });

  it("reads an off-season body as an empty season rather than as a broken one", () => {
    // `server/race-poll/src/body.rs` states it from the other side: a season
    // with no events is a legitimate value, and the pane answers "no races
    // scheduled" rather than rendering a parse failure.
    expect(parseRaceBody(snapshotOf({ events: [] }))).toEqual({ kind: "ok", body: { events: [] } });
  });
});

function inputs(overrides: Partial<QuestionInputs> = {}): QuestionInputs {
  return {
    sync: EMPTY_QUESTION_SYNC,
    bindings: [],
    paneReads: {},
    calendarReads: {},
    calendarConnected: false,
    items: [],
    nowMs: 0,
    ...overrides,
  };
}

function binding(value: BindingDTO["value"]): BindingDTO {
  return { key: BINDING_KEY, known: true, pending: false, value };
}

describe("seriesFromBinding", () => {
  it("reads the comma-separated list exactly as the poller's own binding.rs does", () => {
    // `server/race-poll/src/binding.rs` is the other reader of this one row,
    // and it trims, lowercases, drops blanks and drops repeats while keeping
    // order. A pane that read the same text differently would show a series
    // nothing ever polls, or hide one that is polled.
    expect(seriesFromBinding(" F1 , indycar ,,f1 ")).toEqual(["f1", "indycar"]);
    expect(seriesFromBinding("f1")).toEqual(["f1"]);
    expect(seriesFromBinding(" , , ")).toEqual([]);
  });
});

describe("raceSetup", () => {
  it("tells an unread bindings table apart from a genuinely unset one", () => {
    // Reading `null` as "nobody follows a series" showed every configured
    // device the setup prompt for the whole round trip between mount and the
    // first `bindings` answer — `waste.ts`'s own four-answer lesson.
    expect(raceSetup(inputs({ bindings: null }))).toEqual({ kind: "unread" });
    expect(raceSetup(inputs({ bindings: [] }))).toEqual({ kind: "unset" });
    expect(raceSetup(inputs({ bindings: [binding({ state: "unset" })] }))).toEqual({ kind: "unset" });
    expect(raceSetup(inputs({ bindings: [binding({ state: "text", text: "  " })] }))).toEqual({
      kind: "unset",
    });
  });

  it("reads a row holding something that is not text as a gap, never as unset", () => {
    // A JSON array or number lands as `other` — the binding editor cannot
    // write it, but somebody or something did, and "Not set up" would be a
    // claim about this reader's configuration that is simply false.
    expect(raceSetup(inputs({ bindings: [binding({ state: "other", raw: "[\"f1\"]" })] }))).toEqual({
      kind: "unusable",
    });
  });

  it("hands back every followed series, in the order the binding named them", () => {
    expect(raceSetup(inputs({ bindings: [binding({ state: "text", text: "f1, indycar" })] }))).toEqual(
      { kind: "bound", series: ["f1", "indycar"] },
    );
  });
});

const MINUTE = 60 * 1000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
const NOW = Date.UTC(2026, 7, 10, 9, 0, 0);

function event(name: string, startsAtMs: number, sessionOffsets: number[] = []): unknown {
  return {
    name,
    locality: name.split(" ")[0],
    starts_at_ms: startsAtMs,
    sessions: sessionOffsets.map((offset, index) => ({
      kind: "practice",
      label: `Practice ${index + 1}`,
      starts_at_ms: startsAtMs + offset,
    })),
  };
}

/** Every read-time test goes through the real parser, so a season a test
 * hands the selector is one the wire could actually carry. */
function parseSeason(events: unknown[]): RaceEvent[] {
  const parsed = parseRaceBody(snapshotOf({ events }));
  if (parsed.kind !== "ok") {
    throw new Error(`the test season did not parse: ${parsed.reason}`);
  }
  return parsed.body.events;
}

describe("nextRaceAt", () => {
  it("picks the soonest race still ahead, by instant rather than by feed order", () => {
    const events = [event("Late GP", NOW + 20 * DAY), event("Soon GP", NOW + 3 * DAY)];
    expect(nextRaceAt(parseSeason(events), NOW)?.name).toBe("Soon GP");
  });

  it("treats a race that has already started as behind us, and says why in its own words", () => {
    // **The under-way headline is dropped, deliberately** (#266's grilling
    // note on this issue). Saying "Practice 3 under way" needs a session END
    // time, and the feed publishes none; inventing a per-kind duration is the
    // fabrication rule `city-waste`'s missing `deviation` field already
    // rules out. So the start instant is the horizon — the same reading the
    // lane's own alert takes, since `race-schedule/v1` is registered with
    // `Expiry::Always("the race's start time")`.
    const events = [event("Today GP", NOW + HOUR), event("Next GP", NOW + 14 * DAY)];
    expect(nextRaceAt(parseSeason(events), NOW)?.name).toBe("Today GP");
    expect(nextRaceAt(parseSeason(events), NOW + HOUR + 1)?.name).toBe("Next GP");
  });

  it("has no race to name once the season is over", () => {
    expect(nextRaceAt(parseSeason([event("Done GP", NOW - DAY)]), NOW)).toBeNull();
    expect(nextRaceAt(parseSeason([]), NOW)).toBeNull();
  });
});

describe("countdown", () => {
  it("keeps minutes past the alert's own 90-minute lead, so the two never disagree", () => {
    // The race-start alert (`race_poll::next::LEAD_MS`) fires at 90 minutes
    // and says "in about 90 minutes". A pane that rounded the same instant
    // to "2 hr" would contradict the notification the reader just got.
    expect(countdown(90 * 60_000)).toEqual({ value: "90", unit: "min" });
    expect(countdown(119 * 60_000)).toEqual({ value: "119", unit: "min" });
    expect(countdown(4 * HOUR)).toEqual({ value: "4", unit: "hr" });
    expect(countdown(12 * DAY)).toEqual({ value: "12", unit: "days" });
    expect(countdown(DAY + 12 * HOUR)).toEqual({ value: "36", unit: "hr" });
  });

  it("does not inflect an abbreviation, and does inflect a single day", () => {
    expect(countdown(60_000).unit).toBe("min");
    expect(countdown(2 * DAY)).toEqual({ value: "2", unit: "days" });
    expect(countdown(38 * HOUR)).toEqual({ value: "2", unit: "days" });
  });
});

describe("abbreviate", () => {
  it("renders a Grand Prix as a GP and leaves every other name alone", () => {
    expect(abbreviate("Monaco Grand Prix")).toBe("Monaco GP");
    expect(abbreviate("Iowa 275")).toBe("Iowa 275");
  });
});

function seasonRead(
  entries: Record<string, unknown[] | undefined>,
  alerts: PaneAlertDTO[] = [],
  snapshotOverrides: Partial<PaneSnapshotDTO> = {},
): QuestionInputs["paneReads"] {
  const snapshots = Object.entries(entries).map(([key, events]) => ({
    ...snapshotOf({ events }, snapshotOverrides),
    key,
  }));
  return { [SOURCE]: { source: SOURCE, snapshots, liveAlerts: alerts } };
}

function followed(text: string): BindingDTO[] {
  return [binding({ state: "text", text })];
}

function alert(subjectKey: string | null): PaneAlertDTO {
  return {
    id: `alert-${subjectKey ?? "none"}`,
    subjectKey,
    title: "Monaco Grand Prix starts soon",
    body: "Race start in about 90 minutes.",
    raisedAtMs: NOW,
    expiresAtMs: NOW + 90 * 60_000,
  };
}

describe("raceSubjects", () => {
  it("emits one pane per followed series, and one setup pane when nobody follows any", () => {
    // ADR-0015's "a question registers once and emits 0..N panes", and the
    // acceptance criterion behind it: the list is the binding's, so following
    // a new series takes no code change here.
    expect(raceSubjects(inputs({ bindings: followed("f1, indycar") }))).toEqual(["f1", "indycar"]);
    expect(raceSubjects(inputs({ bindings: [] }))).toEqual([SETUP_SUBJECT]);
    expect(raceSubjects(inputs({ bindings: null }))).toEqual([SETUP_SUBJECT]);
  });
});

describe("raceAnswer", () => {
  it("counts to the race and names it, in the words the issue asked for", () => {
    const answer = raceAnswer("f1", inputs({
      bindings: followed("f1"),
      paneReads: seasonRead({ f1: [event("Monaco Grand Prix", NOW + 12 * DAY, [-2 * DAY])] }),
      nowMs: NOW,
    }));
    expect(answer.answerState).toBe("answered");
    expect(answer.collapsedHeadline).toBe("F1 · Monaco GP in 12 days");
    // `withinBand` is an INSTANT, per the shell contract — the next thing on
    // track, which for a race weekend is Friday practice, not the race.
    expect(answer.withinBand).toBe(NOW + 10 * DAY);
    expect(answer.band).toBe("distant");
  });

  it("renders a series the binding names but nothing has polled as a gap, never as absence", () => {
    // #266 ships an F1 adapter only, so `indycar` in the binding writes no
    // snapshot. Dropping the pane would make adding a series look like it did
    // nothing at all.
    const answer = raceAnswer("indycar", inputs({
      bindings: followed("f1, indycar"),
      paneReads: seasonRead({ f1: [event("Monaco Grand Prix", NOW + DAY)] }),
      nowMs: NOW,
    }));
    expect(answer.answerState).toBe("bound-but-unacquired");
    expect(answer.collapsedHeadline).toContain("IndyCar");
  });

  it("joins an alert on (source, subjectKey) and never shows one series' alert on another's pane", () => {
    // ADR-0015 added `alerts.subject_key` for exactly this, naming this pane
    // as its forcing case: one `source` carries every series, so a join on
    // `source` alone would light up every pane at once.
    const world = (alerts: PaneAlertDTO[]) =>
      inputs({
        bindings: followed("f1, indycar"),
        paneReads: seasonRead(
          {
            f1: [event("Monaco Grand Prix", NOW + 80 * 60_000)],
            indycar: [event("Iowa 275", NOW + 9 * DAY)],
          },
          alerts,
        ),
        nowMs: NOW,
      });

    expect(raceAnswer("f1", world([alert("f1")])).band).toBe("live");
    expect(raceAnswer("indycar", world([alert("f1")])).band).not.toBe("live");
    // An alert naming no subject at all is a legitimate value and belongs to
    // no pane — it still lives in `AlertsScreen`, since the join is additive.
    expect(raceAnswer("f1", world([alert(null)])).band).not.toBe("live");
  });

  it("bands the weekend it is in, and orders two panes inside one band by their next start", () => {
    const world = (startsInMs: number) =>
      inputs({
        bindings: followed("f1"),
        paneReads: seasonRead({ f1: [event("Monaco Grand Prix", NOW + startsInMs, [-2 * DAY])] }),
        nowMs: NOW,
      });
    expect(raceAnswer("f1", world(20 * HOUR)).band).toBe("imminent");
    expect(raceAnswer("f1", world(2 * DAY)).band).toBe("near");
    expect(raceAnswer("f1", world(12 * DAY)).band).toBe("distant");
  });

  it("answers an off-season season rather than calling it a gap", () => {
    const answer = raceAnswer("f1", inputs({
      bindings: followed("f1"),
      paneReads: seasonRead({ f1: [event("Abu Dhabi Grand Prix", NOW - 3 * DAY)] }),
      nowMs: NOW,
    }));
    expect(answer.answerState).toBe("answered");
    expect(answer.band).toBe("dormant");
    expect(answer.withinBand).toBeNull();
    expect(answer.collapsedHeadline).toBe("F1 · No races scheduled");
  });

  it("says Not set up only for a genuinely unset binding, and Checking setup while it is unread", () => {
    expect(raceAnswer(SETUP_SUBJECT, inputs({ bindings: [] })).answerState).toBe("unbound");
    expect(raceAnswer(SETUP_SUBJECT, inputs({ bindings: null })).answerState).toBe(
      "bound-but-unacquired",
    );
  });
});

describe("raceView", () => {
  it("calls an answer stale past twelve hours, and never calls an unknown age fresh", () => {
    const world = (freshness: PaneSnapshotDTO["freshness"]) =>
      inputs({
        bindings: followed("f1"),
        paneReads: seasonRead({ f1: [event("Monaco Grand Prix", NOW + 12 * DAY)] }, [], {
          freshness,
        }),
        nowMs: NOW,
      });
    expect(raceView("f1", world({ kind: "age", ageMs: 11 * HOUR, declaredCadenceMs: null }))?.stale).toBe(
      false,
    );
    expect(raceView("f1", world({ kind: "age", ageMs: 13 * HOUR, declaredCadenceMs: null }))?.stale).toBe(
      true,
    );
    expect(raceView("f1", world({ kind: "unknown" }))?.stale).toBe(true);
  });
});

describe("raceHeadlineParts", () => {
  const view = (deltaMs: number) =>
    raceView(
      "f1",
      inputs({
        bindings: followed("f1"),
        paneReads: seasonRead({ f1: [event("Monaco Grand Prix", NOW + deltaMs)] }),
        nowMs: NOW,
      }),
    )!;

  it("splits the race from the count at every unit, minutes included", () => {
    // The split is about which words are the answer, and that does not change
    // when the answer gets close — the vacation pane sets its own countdown
    // the same way at every distance.
    expect(raceHeadlineParts(view(12 * DAY), NOW)).toEqual({
      kind: "countdown",
      name: "Monaco GP",
      value: "12",
      unit: "days",
    });
    expect(raceHeadlineParts(view(5 * HOUR), NOW)).toEqual({
      kind: "countdown",
      name: "Monaco GP",
      value: "5",
      unit: "hr",
    });
    expect(raceHeadlineParts(view(40 * MINUTE), NOW)).toEqual({
      kind: "countdown",
      name: "Monaco GP",
      value: "40",
      unit: "min",
    });
  });

  it("falls back to a sentence when there is no race to count to", () => {
    // Off-season has no name and no number, so there is nothing to set apart —
    // it is one string, and the pane renders it as one.
    expect(raceHeadlineParts(view(-3 * DAY), NOW)).toEqual({
      kind: "fallback",
      text: "No races scheduled",
    });
  });
});

describe("dayLabel", () => {
  it("names the device's own day, with no zone label anywhere", () => {
    // ADR-0015: device-local everywhere, and the prototype's hardcoded
    // `America/Los_Angeles` plus its "PT" suffix both go. Rollover is
    // compared as local calendar days, never as elapsed milliseconds — a
    // 23:00 race and a 01:00 "now" are two hours and two days apart.
    const localNoon = new Date(2026, 7, 10, 12, 0, 0).getTime();
    expect(dayLabel(localNoon, localNoon)).toBe("Today");
    expect(dayLabel(new Date(2026, 7, 11, 1, 0, 0).getTime(), localNoon)).toBe("Tomorrow");
    expect(dayLabel(new Date(2026, 7, 13, 12, 0, 0).getTime(), localNoon)).toBe("Thursday");
    expect(dayLabel(new Date(2026, 8, 20, 12, 0, 0).getTime(), localNoon)).toMatch(/Sep/);
    expect(clock(new Date(2026, 7, 10, 16, 30, 0).getTime())).not.toMatch(/PT|UTC|GMT/);
  });
});
