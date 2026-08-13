import { describe, expect, it } from "vitest";
import type { FreshnessDTO, PaneReadDTO, PaneSnapshotDTO } from "../../store/protocol";
import type { QuestionInputs } from "../questions/contract";
import {
  IMMINENT_THRESHOLD_USD,
  NEAR_THRESHOLD_USD,
  SNAPSHOT_KEY,
  SOURCE,
  STALE_AFTER_MS,
  formatUsd,
  isStaleFreshness,
  kimiAnswer,
  kimiBand,
  kimiCollapsedHeadline,
  kimiGapReason,
  kimiView,
  parseKimiBody,
} from "./kimi";

// The pane #313 was built to prove — the first poller-backed, non-gap
// Status pane, per the batch's own review point. Everything here is pure:
// the clock arrives on `QuestionInputs`, exactly like `waste.test.ts`.

const NOW = Date.parse("2026-08-12T16:00:00Z");

function body(overrides: Partial<{ available: number; voucher: number; cash: number }> = {}): string {
  return JSON.stringify({
    available_balance: overrides.available ?? 12.4,
    voucher_balance: overrides.voucher ?? 8.0,
    cash_balance: overrides.cash ?? 4.4,
  });
}

function snapshot(overrides: Partial<PaneSnapshotDTO> = {}): PaneSnapshotDTO {
  return {
    key: SNAPSHOT_KEY,
    fetchedAtMs: NOW - 60_000,
    envelope: { kind: "ok", schema: SOURCE, polledEveryMs: 21_600_000, body: body() },
    freshness: { kind: "age", ageMs: 60_000, declaredCadenceMs: 21_600_000 },
    ...overrides,
  };
}

function read(snapshots: PaneSnapshotDTO[] = [snapshot()]): PaneReadDTO {
  return { source: SOURCE, snapshots, liveAlerts: [] };
}

function inputs(overrides: Partial<QuestionInputs> = {}): QuestionInputs {
  return {
    bindings: [],
    paneReads: { [SOURCE]: read() },
    calendarReads: {},
    calendarConnected: false,
    items: [],
    nowMs: NOW,
    ...overrides,
  };
}

describe("parseKimiBody", () => {
  it("reads a well-formed body", () => {
    const parsed = parseKimiBody(snapshot());
    expect(parsed).toEqual({
      kind: "ok",
      body: { availableBalance: 12.4, voucherBalance: 8.0, cashBalance: 4.4 },
    });
  });

  it("reads a negative cash balance without rejecting it", () => {
    const parsed = parseKimiBody(snapshot({ envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ available: 4.1, voucher: 5.1, cash: -1 }) } }));
    expect(parsed).toEqual({
      kind: "ok",
      body: { availableBalance: 4.1, voucherBalance: 5.1, cashBalance: -1 },
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
          envelope: {
            kind: "ok",
            schema: SOURCE,
            polledEveryMs: null,
            body: JSON.stringify({ available_balance: "not a number", voucher_balance: 1, cash_balance: 1 }),
          },
        }),
        /numbers can't be read/,
      ],
    ];
    for (const [row, pattern] of cases) {
      const parsed = parseKimiBody(row);
      expect(parsed.kind).toBe("gap");
      if (parsed.kind === "gap") {
        expect(parsed.reason).toMatch(pattern);
      }
    }
  });

  it("reads an unrecognised schema as this device being behind, not as a broken payload", () => {
    const parsed = parseKimiBody(
      snapshot({ envelope: { kind: "ok", schema: "kimi-balance/v9", polledEveryMs: null, body: body() } }),
    );
    expect(parsed.kind).toBe("gap");
    if (parsed.kind === "gap") {
      expect(parsed.reason).toMatch(/Update the app/);
    }
  });
});

describe("kimiBand", () => {
  it("bands purely off available_balance, as distance from the $0 cliff", () => {
    expect(kimiBand(0)).toBe("live");
    expect(kimiBand(-3)).toBe("live");
    expect(kimiBand(0.5)).toBe("imminent");
    expect(kimiBand(IMMINENT_THRESHOLD_USD)).toBe("near");
    expect(kimiBand(3)).toBe("near");
    expect(kimiBand(NEAR_THRESHOLD_USD)).toBe("dormant");
    expect(kimiBand(50)).toBe("dormant");
  });
});

describe("formatUsd", () => {
  it("puts the sign in front of the symbol", () => {
    expect(formatUsd(4.1)).toBe("$4.10");
    expect(formatUsd(-1)).toBe("-$1.00");
    expect(formatUsd(0)).toBe("$0.00");
  });
});

describe("kimiCollapsedHeadline", () => {
  it("carries the decision, not just the datum", () => {
    expect(kimiCollapsedHeadline(50)).toBe("$50.00 left");
    expect(kimiCollapsedHeadline(3)).toBe("$3.00 — running low");
    expect(kimiCollapsedHeadline(0.5)).toBe("$0.50 — critical");
    expect(kimiCollapsedHeadline(0)).toBe("$0.00 — exhausted");
    expect(kimiCollapsedHeadline(-2)).toBe("-$2.00 — exhausted");
  });
});

describe("kimiAnswer", () => {
  it("answers with the balance, banded, when a snapshot has landed", () => {
    const answer = kimiAnswer(inputs());
    expect(answer.answerState).toBe("answered");
    expect(answer.band).toBe("dormant");
    expect(answer.withinBand).toBeNull();
    expect(answer.collapsedHeadline).toBe("$12.40 left");
  });

  it("is a gap, never an unbound question, when nothing has polled yet", () => {
    const answer = kimiAnswer(inputs({ paneReads: {} }));
    expect(answer.answerState).toBe("bound-but-unacquired");
    expect(answer.band).toBe("dormant");
    expect(answer.collapsedHeadline).toBe("No answer yet");
  });

  it("bands live and shows a siren glyph once the balance hits the cliff", () => {
    const answer = kimiAnswer(
      inputs({ paneReads: { [SOURCE]: read([snapshot({ envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ available: 0 }) } })]) } } ),
    );
    expect(answer.band).toBe("live");
    expect(answer.icon?.[0]).toEqual({ kind: "icon", name: "siren", label: "kimi balance exhausted" });
  });
});

describe("kimiView / kimiGapReason", () => {
  it("returns null / the reason together, never disagreeing", () => {
    const emptyInputs = inputs({ paneReads: {} });
    expect(kimiView(emptyInputs)).toBeNull();
    expect(kimiGapReason(emptyInputs)).toMatch(/hasn't been fetched|has been fetched|No balance answer yet/i);
  });

  it("surfaces the negative-cash split on the resolved view", () => {
    const view = kimiView(
      inputs({
        paneReads: {
          [SOURCE]: read([
            snapshot({ envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ available: 4.1, voucher: 5.1, cash: -1 }) } }),
          ]),
        },
      }),
    );
    expect(view?.body.cashBalance).toBe(-1);
  });
});

describe("isStaleFreshness / staleness", () => {
  it("never reads unknown as fresh, against this pane's own threshold", () => {
    const unknown: FreshnessDTO = { kind: "unknown" };
    expect(isStaleFreshness(unknown, STALE_AFTER_MS)).toBe(true);
  });

  it("bands stale at ~13h against the 6h cadence — two missed polls", () => {
    const freshRead = read([snapshot({ freshness: { kind: "age", ageMs: 12 * 60 * 60 * 1000, declaredCadenceMs: 21_600_000 } })]);
    const staleRead = read([snapshot({ freshness: { kind: "age", ageMs: 14 * 60 * 60 * 1000, declaredCadenceMs: 21_600_000 } })]);

    const freshView = kimiView(inputs({ paneReads: { [SOURCE]: freshRead } }));
    const staleView = kimiView(inputs({ paneReads: { [SOURCE]: staleRead } }));
    expect(freshView?.stale).toBe(false);
    expect(staleView?.stale).toBe(true);
  });
});
