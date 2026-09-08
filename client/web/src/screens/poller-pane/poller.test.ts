import { describe, expect, it } from "vitest";
import type { PaneReadDTO, PaneSnapshotDTO } from "../../store/protocol";
import { EMPTY_QUESTION_SYNC, type QuestionInputs } from "../questions/contract";
import {
  FLOOR_MS,
  OVERDUE_MULTIPLIER,
  SOURCES,
  pollerAnswer,
  pollerBand,
  pollerCollapsedHeadline,
  pollerGapReason,
  pollerGlyph,
  pollerSubjects,
  pollerView,
} from "./poller";

// #775's own pane test, `uptime.test.ts`'s own shape — this pane's own
// twist is that it touches no body at all, so every fixture here is a
// freshness alone.

const NOW = Date.parse("2026-08-12T16:00:00Z");
const KIMI = "kimi-balance/v1";

function snapshot(key: string, overrides: Partial<PaneSnapshotDTO> = {}): PaneSnapshotDTO {
  return {
    key,
    fetchedAtMs: NOW - 60_000,
    envelope: { kind: "ok", schema: KIMI, polledEveryMs: 21_600_000, body: "{}" },
    freshness: { kind: "age", ageMs: 60_000, declaredCadenceMs: 21_600_000 },
    ...overrides,
  };
}

function read(source: string, snapshots: PaneSnapshotDTO[]): PaneReadDTO {
  return { source, snapshots, liveAlerts: [] };
}

function inputs(overrides: Partial<QuestionInputs> = {}): QuestionInputs {
  return {
    sync: EMPTY_QUESTION_SYNC,
    bindings: [],
    paneReads: { [KIMI]: read(KIMI, [snapshot("balance")]) },
    calendarReads: {},
    calendarConnected: false,
    items: [],
    nowMs: NOW,
    ...overrides,
  };
}

describe("SOURCES", () => {
  it("is nine sources — every registered snapshot-writing one but the retired v1", () => {
    expect(SOURCES).toHaveLength(9);
    expect(SOURCES).toContain("uptime/v1");
    expect(SOURCES).not.toContain("city-waste/v1");
  });
});

describe("pollerBand", () => {
  it("bands a fresh row dormant", () => {
    expect(pollerBand({ kind: "age", ageMs: 60_000, declaredCadenceMs: 900_000 })).toBe("dormant");
  });

  it("bands a row past the multiplied cadence imminent", () => {
    const cadence = 15 * 60 * 1000;
    expect(
      pollerBand({ kind: "age", ageMs: cadence * OVERDUE_MULTIPLIER + 1, declaredCadenceMs: cadence }),
    ).toBe("imminent");
  });

  it("floors the overdue threshold for a very tight cadence", () => {
    const cadence = 60_000;
    expect(pollerBand({ kind: "age", ageMs: FLOOR_MS - 1, declaredCadenceMs: cadence })).toBe("dormant");
    expect(pollerBand({ kind: "age", ageMs: FLOOR_MS + 1, declaredCadenceMs: cadence })).toBe("imminent");
  });

  it("never reads a cadence-less row as healthy", () => {
    const band = pollerBand({ kind: "age", ageMs: 1_000, declaredCadenceMs: null });
    expect(band).toBe("distant");
    expect(band).not.toBe("dormant");
  });

  it("reads unknown freshness as the most severe band, never dormant", () => {
    const band = pollerBand({ kind: "unknown" });
    expect(band).toBe("imminent");
    expect(band).not.toBe("dormant");
  });
});

describe("pollerSubjects", () => {
  it("is every watched source, always — even with nothing read at all", () => {
    expect(pollerSubjects(inputs({ paneReads: {} }))).toEqual(SOURCES);
  });

  it("is still every watched source once some have data", () => {
    expect(pollerSubjects(inputs())).toEqual(SOURCES);
  });
});

describe("pollerCollapsedHeadline / pollerGlyph", () => {
  it("names the source and the band, distinctly per reading", () => {
    expect(
      pollerCollapsedHeadline({ source: KIMI, band: "dormant", freshness: { kind: "age", ageMs: 60_000, declaredCadenceMs: 21_600_000 } }),
    ).toMatch(/^kimi-balance\/v1 · healthy, last row/);
    expect(
      pollerCollapsedHeadline({ source: KIMI, band: "imminent", freshness: { kind: "age", ageMs: 90_000_000, declaredCadenceMs: 21_600_000 } }),
    ).toMatch(/^kimi-balance\/v1 · overdue, last row/);
    expect(
      pollerCollapsedHeadline({ source: KIMI, band: "distant", freshness: { kind: "age", ageMs: 60_000, declaredCadenceMs: null } }),
    ).toMatch(/^kimi-balance\/v1 · cadence unreadable, last row/);
    expect(pollerCollapsedHeadline({ source: KIMI, band: "imminent", freshness: { kind: "unknown" } })).toBe(
      "kimi-balance/v1 · age unknown",
    );
  });

  it("names the source in the glyph label", () => {
    const glyph = pollerGlyph({ source: KIMI, band: "imminent", freshness: { kind: "unknown" } });
    expect(glyph).toEqual({ kind: "icon", name: "siren", label: `${KIMI} overdue` });
  });
});

describe("pollerAnswer", () => {
  it("is a gap, distinctly named per source, never an unbound question, when nothing has been read", () => {
    const answer = pollerAnswer(KIMI, inputs({ paneReads: {} }));
    expect(answer.answerState).toBe("bound-but-unacquired");
    expect(answer.band).toBe("dormant");
    expect(answer.collapsedHeadline).toBe(`${KIMI} · No answer yet`);
  });

  it("answers with the source's own band once a row has landed", () => {
    const answer = pollerAnswer(KIMI, inputs());
    expect(answer.answerState).toBe("answered");
    expect(answer.band).toBe("dormant");
  });

  it("answers imminent for a source that has gone quiet past its own cadence", () => {
    const cadence = 21_600_000;
    const rows = read(KIMI, [
      snapshot("balance", { freshness: { kind: "age", ageMs: cadence * OVERDUE_MULTIPLIER + 1, declaredCadenceMs: cadence } }),
    ]);
    const answer = pollerAnswer(KIMI, inputs({ paneReads: { [KIMI]: rows } }));
    expect(answer.band).toBe("imminent");
  });

  it("resolves off the freshest row when a source carries many keys", () => {
    const github = "github-hummingbird/v1";
    const rows = read(github, [
      snapshot("stale.yml", {
        envelope: { kind: "ok", schema: github, polledEveryMs: 1_800_000, body: "{}" },
        freshness: { kind: "age", ageMs: 3_600_000, declaredCadenceMs: 1_800_000 },
      }),
      snapshot("fresh.yml", {
        envelope: { kind: "ok", schema: github, polledEveryMs: 1_800_000, body: "{}" },
        freshness: { kind: "age", ageMs: 60_000, declaredCadenceMs: 1_800_000 },
      }),
    ]);
    const answer = pollerAnswer(github, inputs({ paneReads: { [github]: rows } }));
    expect(answer.band).toBe("dormant");
  });
});

describe("pollerView / pollerGapReason", () => {
  it("returns null / the reason together, never disagreeing", () => {
    const emptyInputs = inputs({ paneReads: {} });
    expect(pollerView(KIMI, emptyInputs)).toBeNull();
    expect(pollerGapReason(KIMI, emptyInputs)).toMatch(/fetched yet/i);
  });

  it("carries the source's band and freshness once acquired", () => {
    const view = pollerView(KIMI, inputs());
    expect(view).toEqual({
      source: KIMI,
      band: "dormant",
      freshness: { kind: "age", ageMs: 60_000, declaredCadenceMs: 21_600_000 },
    });
  });
});
