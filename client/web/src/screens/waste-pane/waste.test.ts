import { describe, expect, it } from "vitest";
import type { BindingDTO, FreshnessDTO, PaneReadDTO, PaneSnapshotDTO } from "../../store/protocol";
import type { QuestionInputs } from "../questions/contract";
import {
  BINDING_KEY,
  SNAPSHOT_KEY,
  SOURCE,
  STALE_AFTER_MS,
  isStaleFreshness,
  parseWasteBody,
  wasteAnswer,
  wasteCollapsedHeadline,
  wasteGapReason,
  wasteGlyphs,
  wasteHeadline,
  wasteView,
} from "./waste";

// The pane the shell was built to prove (#120 over #245). Everything here is
// pure: the clock arrives on `QuestionInputs`, and every day-shaped answer is
// resolved in the payload's own zone.

const ZONE = "America/Los_Angeles";
/** Monday 2026-08-10, 09:00 at the address. */
const NOW = Date.parse("2026-08-10T16:00:00Z");

function body(overrides: Partial<{ zone: string; scheduled: string; collectedOn: string; streams: string[] }> = {}): string {
  return JSON.stringify({
    zone: overrides.zone ?? ZONE,
    scheduled: overrides.scheduled ?? "2026-08-17",
    collected_on: overrides.collectedOn ?? "2026-08-17",
    streams: overrides.streams ?? ["trash", "yard"],
  });
}

function snapshot(overrides: Partial<PaneSnapshotDTO> = {}): PaneSnapshotDTO {
  return {
    key: SNAPSHOT_KEY,
    fetchedAtMs: NOW - 60_000,
    envelope: { kind: "ok", schema: SOURCE, polledEveryMs: 86_400_000, body: body() },
    freshness: { kind: "age", ageMs: 60_000, declaredCadenceMs: 86_400_000 },
    ...overrides,
  };
}

function read(snapshots: PaneSnapshotDTO[] = [snapshot()]): PaneReadDTO {
  return { source: SOURCE, snapshots, liveAlerts: [] };
}

function boundBinding(): BindingDTO {
  return {
    key: BINDING_KEY,
    known: true,
    pending: false,
    value: { state: "text", text: "https://example.gov/collection" },
  };
}

function inputs(overrides: Partial<QuestionInputs> = {}): QuestionInputs {
  return {
    bindings: [boundBinding()],
    paneReads: { [SOURCE]: read() },
    calendarReads: {},
    nowMs: NOW,
    ...overrides,
  };
}

describe("parseWasteBody", () => {
  it("reads a well-formed body", () => {
    const parsed = parseWasteBody(snapshot());
    expect(parsed).toEqual({
      kind: "ok",
      body: { zone: ZONE, scheduled: "2026-08-17", collectedOn: "2026-08-17", streams: ["trash", "yard"] },
    });
  });

  it("says why it could not, rather than answering emptily", () => {
    const cases: [PaneSnapshotDTO | undefined, RegExp][] = [
      [undefined, /hasn't been fetched|has been fetched/i],
      [snapshot({ envelope: { kind: "malformed", reason: "`body` is missing" } }), /couldn't be read/],
      [snapshot({ envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: "{" } }), /isn't JSON/],
      [snapshot({ envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: "[]" } }), /isn't an object/],
      [
        snapshot({
          envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ zone: "" }) },
        }),
        /time zone/,
      ],
      [
        snapshot({
          envelope: {
            kind: "ok",
            schema: SOURCE,
            polledEveryMs: null,
            body: body({ collectedOn: "next tuesday" }),
          },
        }),
        /whole days/,
      ],
      [
        snapshot({
          envelope: {
            kind: "ok",
            schema: SOURCE,
            polledEveryMs: null,
            body: body({ streams: ["trash", "nuclear"] }),
          },
        }),
        /unknown kind of bin/,
      ],
      [
        snapshot({
          envelope: {
            kind: "ok",
            schema: SOURCE,
            polledEveryMs: null,
            body: body({ zone: "Mars/Olympus_Mons" }),
          },
        }),
        /unknown time zone/,
      ],
    ];
    for (const [row, pattern] of cases) {
      const parsed = parseWasteBody(row);
      expect(parsed.kind).toBe("gap");
      if (parsed.kind === "gap") {
        expect(parsed.reason).toMatch(pattern);
      }
    }
  });

  it("reads an unrecognised schema as this device being behind, not as a broken payload", () => {
    const parsed = parseWasteBody(
      snapshot({
        envelope: { kind: "ok", schema: "city-waste/v9", polledEveryMs: null, body: body() },
      }),
    );
    expect(parsed.kind).toBe("gap");
    if (parsed.kind === "gap") {
      expect(parsed.reason).toMatch(/Update the app/);
    }
  });
});

describe("isStaleFreshness", () => {
  it("never reads unknown as fresh, against any threshold", () => {
    const unknown: FreshnessDTO = { kind: "unknown" };
    expect(isStaleFreshness(unknown, STALE_AFTER_MS)).toBe(true);
    expect(isStaleFreshness(unknown, Number.MAX_SAFE_INTEGER)).toBe(true);
  });

  it("calls 25h fresh and 27h stale — the line is the cost of a wrong answer, not 2x the cadence", () => {
    const aged = (hours: number): FreshnessDTO => ({
      kind: "age",
      ageMs: hours * 3_600_000,
      declaredCadenceMs: 86_400_000,
    });
    expect(isStaleFreshness(aged(25), STALE_AFTER_MS)).toBe(false);
    expect(isStaleFreshness(aged(27), STALE_AFTER_MS)).toBe(true);
  });
});

describe("the headlines", () => {
  it("says when, because the bins already say which", () => {
    expect(wasteHeadline(0, "Monday", false)).toBe("Trash Today");
    expect(wasteHeadline(1, "Tuesday", false)).toBe("Trash Tonight");
    expect(wasteHeadline(3, "Thursday", false)).toBe("Trash Thursday");
  });

  it("names a holiday's actual day even when that day is tomorrow", () => {
    // On the one week the day is unusual, "Tonight" would hide exactly the
    // thing that changed.
    expect(wasteHeadline(1, "Tuesday", true)).toBe("Trash Tuesday");
  });

  it("says 'today' only about today, never about a day already gone", () => {
    // `<= 0` here is what rendered yesterday's collection as today's; the
    // pane refuses a past collection outright, and these read as equality so
    // a future caller cannot reintroduce it.
    expect(wasteHeadline(-1, "Sunday", false)).not.toMatch(/today/i);
    expect(wasteCollapsedHeadline(-1, "Sunday", false)).not.toMatch(/today/i);
  });

  it("collapses to one line, and a dormant pane's line names the day and the distance", () => {
    expect(wasteCollapsedHeadline(0, "Monday", false)).toBe("Trash today");
    expect(wasteCollapsedHeadline(1, "Tuesday", false)).toBe("Trash tonight");
    expect(wasteCollapsedHeadline(1, "Tuesday", true)).toBe("Tuesday · 1d");
    expect(wasteCollapsedHeadline(4, "Friday", false)).toBe("Friday · 4d");
  });
});

describe("wasteGlyphs", () => {
  it("emits one labelled dot per bin, in kerb order rather than payload order", () => {
    const glyphs = wasteGlyphs(["yard", "trash"]);
    expect(glyphs.map((glyph) => (glyph.kind === "dot" ? glyph.label : glyph.name))).toEqual([
      "trash",
      "yard",
    ]);
    // Colour alone is not a label to a screen reader.
    expect(glyphs.every((glyph) => glyph.label !== "")).toBe(true);
  });
});

describe("wasteAnswer", () => {
  it("is unbound when nobody has set the page, and still renders a pane", () => {
    // A question that vanished while unbound is a question nobody could ever
    // discover — so it stays, with its setup prompt.
    const answer = wasteAnswer(inputs({ bindings: [] }));
    expect(answer.answerState).toBe("unbound");
    expect(answer.collapsedHeadline).toBe("Not set up");
  });

  it("is a gap, not 'not set up', when the binding holds something that is not text", () => {
    // A row that exists but cannot be used is something the reader can act
    // on. "Not set up" would describe a state this device can see is false.
    const answer = wasteAnswer(
      inputs({ bindings: [{ ...boundBinding(), value: { state: "other", raw: "7" } }] }),
    );
    expect(answer.answerState).toBe("bound-but-unacquired");
    expect(answer.collapsedHeadline).toBe("Setup needs a look");
  });

  it("is a gap, not 'not set up', before the bindings table has been read at all", () => {
    // `bindings: null` is every device's state for the round-trip between
    // mount and the first `bindings` answer — reading it as unbound showed a
    // configured reader the setup prompt on every single load.
    const answer = wasteAnswer(inputs({ bindings: null }));
    expect(answer.answerState).toBe("bound-but-unacquired");
    expect(answer.collapsedHeadline).toBe("Checking setup");
  });

  it("reads a blanked binding as never set — `settings` has no DELETE", () => {
    const answer = wasteAnswer(
      inputs({ bindings: [{ ...boundBinding(), value: { state: "text", text: "   " } }] }),
    );
    expect(answer.answerState).toBe("unbound");
  });

  it("is a gap — not an absence — when bound but nothing has landed", () => {
    for (const paneReads of [{}, { [SOURCE]: read([]) }]) {
      expect(wasteAnswer(inputs({ paneReads })).answerState).toBe("bound-but-unacquired");
    }
    const malformed = read([snapshot({ envelope: { kind: "malformed", reason: "boom" } })]);
    expect(wasteAnswer(inputs({ paneReads: { [SOURCE]: malformed } })).answerState).toBe(
      "bound-but-unacquired",
    );
  });

  it("is answered, dormant and still ordered four days out", () => {
    // Dormant does not mean unordered: a real `withinBand` is what keeps two
    // quiet panes in a meaningful order rather than an alphabetical one.
    const answer = wasteAnswer(inputs());
    expect(answer.answerState).toBe("answered");
    expect(answer.band).toBe("dormant");
    // An absolute instant — midnight at the address — never a duration: the
    // sort reads no clock, and the value cannot age between renders.
    expect(answer.withinBand).toBe(Date.parse("2026-08-17T07:00:00Z"));
    expect(answer.collapsedHeadline).toBe("Monday · 7d");
    expect(answer.icon).toHaveLength(2);
  });

  it("is imminent on the eve, on the day itself, and every day of a holiday week", () => {
    const eve = wasteAnswer(
      inputs({ paneReads: { [SOURCE]: read([snapshot({
        envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ scheduled: "2026-08-11", collectedOn: "2026-08-11" }) },
      })]) } }),
    );
    expect(eve.band).toBe("imminent");

    const today = wasteAnswer(
      inputs({ paneReads: { [SOURCE]: read([snapshot({
        envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ scheduled: "2026-08-10", collectedOn: "2026-08-10" }) },
      })]) } }),
    );
    expect(today.band).toBe("imminent");
    // Already past on the day itself — which is what sorts today ahead of
    // tomorrow inside the band.
    expect(today.withinBand).toBeLessThan(NOW);

    // Four days out, but the day has moved: awake all week, because the
    // pane is the only signal a holiday ever gets.
    const holiday = wasteAnswer(
      inputs({ paneReads: { [SOURCE]: read([snapshot({
        envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ scheduled: "2026-08-13", collectedOn: "2026-08-14" }) },
      })]) } }),
    );
    expect(holiday.band).toBe("imminent");
    expect(holiday.collapsedHeadline).toBe("Friday · 4d");
  });

  it("reads a holiday off the snapshot and never off the alert lane", () => {
    // The alert row still exists for the notification lane (ADR-0012); this
    // pane deliberately does not join it, because a holiday IS the answer
    // and there is nothing to acknowledge away.
    const withAlert: PaneReadDTO = {
      ...read([snapshot({
        envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ scheduled: "2026-08-13", collectedOn: "2026-08-14" }) },
      })]),
      liveAlerts: [
        {
          id: "alert-1",
          subjectKey: SNAPSHOT_KEY,
          title: "Collection: Thursday -> Friday this week",
          body: null,
          raisedAtMs: NOW - 1_000,
          expiresAtMs: null,
        },
      ],
    };
    const withAlertAnswer = wasteAnswer(inputs({ paneReads: { [SOURCE]: withAlert } }));
    const withoutAlertAnswer = wasteAnswer(
      inputs({ paneReads: { [SOURCE]: { ...withAlert, liveAlerts: [] } } }),
    );
    expect(withAlertAnswer).toEqual(withoutAlertAnswer);
    expect(wasteView(inputs({ paneReads: { [SOURCE]: withAlert } }))?.holiday).toBe(true);
  });

  it("resolves the day at the address, not on the device", () => {
    // 2026-08-11T03:00Z is still Monday the 10th in Los Angeles, so the
    // Tuesday collection is one day away — "tonight", not "today".
    const late = Date.parse("2026-08-11T03:00:00Z");
    const view = wasteView(
      inputs({
        nowMs: late,
        paneReads: { [SOURCE]: read([snapshot({
          envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ scheduled: "2026-08-11", collectedOn: "2026-08-11" }) },
        })]) },
      }),
    );
    expect(view?.today).toBe("2026-08-10");
    expect(view?.daysAway).toBe(1);
    expect(view?.weekday).toBe("Tuesday");
  });

  it("never presents a collection that has already happened as today's", () => {
    // The window this closes is ordinary, not exotic: the poll is daily, so
    // from the address's midnight until that day's fetch the snapshot still
    // names yesterday's collection — and at ~9 hours old it is nowhere near
    // the 26h stale line, so freshness says nothing about it.
    const yesterday = read([
      snapshot({
        envelope: {
          kind: "ok",
          schema: SOURCE,
          polledEveryMs: 86_400_000,
          body: body({ scheduled: "2026-08-09", collectedOn: "2026-08-09" }),
        },
      }),
    ]);
    const stale = inputs({ paneReads: { [SOURCE]: yesterday } });

    expect(wasteView(stale)).toBeNull();
    expect(wasteGapReason(stale)).toMatch(/out of date.*Sunday 2026-08-09.*passed/);

    const answer = wasteAnswer(stale);
    expect(answer.answerState).toBe("bound-but-unacquired");
    // Not imminent, and above all not "Trash today".
    expect(answer.band).toBe("dormant");
    expect(answer.collapsedHeadline).toBe("No answer yet");
  });

  it("marks an answer stale past the threshold without losing it", () => {
    const stale = wasteView(
      inputs({
        paneReads: {
          [SOURCE]: read([snapshot({ freshness: { kind: "unknown" } })]),
        },
      }),
    );
    // Still an answer — the brand rule is to keep showing the data and say
    // its age, never to hide it.
    expect(stale?.stale).toBe(true);
    expect(stale?.daysAway).toBe(7);
  });
});
