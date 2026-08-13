import { describe, expect, it } from "vitest";
import type { FreshnessDTO, PaneReadDTO, PaneSnapshotDTO } from "../../store/protocol";
import type { QuestionInputs } from "../questions/contract";
import {
  NEVER_POLLED_SUBJECT,
  SOURCE,
  STALE_AFTER_MS,
  isStaleFreshness,
  parseUptimeBody,
  uptimeAnswer,
  uptimeBand,
  uptimeCollapsedHeadline,
  uptimeGapReason,
  uptimeGlyph,
  uptimeSubjects,
  uptimeView,
} from "./uptime";

// #315's own pane test, `github.test.ts`'s own shape: everything here is
// pure, the clock arrives on `QuestionInputs`.

const NOW = Date.parse("2026-08-12T16:00:00Z");

function body(
  overrides: Partial<{ expected: "on" | "off"; expectStatus: number; observedStatus: number | null; error: string | null }> = {},
): string {
  return JSON.stringify({
    expected: overrides.expected ?? "on",
    expect_status: overrides.expectStatus ?? 401,
    // `??` treats an explicit `null` override as absent, which is exactly
    // the value this helper's callers need to pass through untouched (an
    // unreachable reading is `observedStatus: null`) — `"observedStatus" in
    // overrides` is what actually distinguishes "not passed" from "passed
    // as null".
    observed_status: "observedStatus" in overrides ? overrides.observedStatus : 401,
    error: "error" in overrides ? overrides.error : null,
  });
}

function snapshot(key: string, overrides: Partial<PaneSnapshotDTO> = {}): PaneSnapshotDTO {
  return {
    key,
    fetchedAtMs: NOW - 5 * 60_000,
    envelope: { kind: "ok", schema: SOURCE, polledEveryMs: 3_600_000, body: body() },
    freshness: { kind: "age", ageMs: 5 * 60_000, declaredCadenceMs: 3_600_000 },
    ...overrides,
  };
}

function read(snapshots: PaneSnapshotDTO[]): PaneReadDTO {
  return { source: SOURCE, snapshots, liveAlerts: [] };
}

function inputs(overrides: Partial<QuestionInputs> = {}): QuestionInputs {
  return {
    bindings: [],
    paneReads: { [SOURCE]: read([snapshot("authority")]) },
    calendarReads: {},
    calendarConnected: false,
    items: [],
    nowMs: NOW,
    ...overrides,
  };
}

describe("parseUptimeBody", () => {
  it("reads a well-formed body", () => {
    const parsed = parseUptimeBody(snapshot("authority"));
    expect(parsed).toEqual({
      kind: "ok",
      body: { expected: "on", expectStatus: 401, observedStatus: 401, error: null },
    });
  });

  it("reads an unreachable body, error present, status absent", () => {
    const parsed = parseUptimeBody(
      snapshot("runner", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ observedStatus: null, error: "connection refused" }) } }),
    );
    expect(parsed).toEqual({
      kind: "ok",
      body: { expected: "on", expectStatus: 401, observedStatus: null, error: "connection refused" },
    });
  });

  it("says why it could not, rather than answering emptily — every distinct malformation gets its own reason", () => {
    const cases: [PaneSnapshotDTO | undefined, RegExp][] = [
      [undefined, /fetched yet/],
      [snapshot("x", { envelope: { kind: "malformed", reason: "`body` is missing" } }), /couldn't be read/],
      [snapshot("x", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: "{" } }), /isn't JSON/],
      [snapshot("x", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: "[]" } }), /isn't an object/],
      [
        snapshot("x", {
          envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: JSON.stringify({ expected: "maybe", expect_status: 401, observed_status: 401, error: null }) },
        }),
        /fields can't be read/,
      ],
      [
        // Both an observed_status AND an error — the wire never produces
        // this, and reading it as anything but a gap would be inventing a
        // third reading nothing on the server side ever means.
        snapshot("x", {
          envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ observedStatus: 401, error: "also this" }) },
        }),
        /observation can't be read/,
      ],
    ];
    for (const [row, pattern] of cases) {
      const parsed = parseUptimeBody(row);
      expect(parsed.kind).toBe("gap");
      if (parsed.kind === "gap") {
        expect(parsed.reason).toMatch(pattern);
      }
    }
  });

  it("reads an unrecognised schema as this device being behind, not as a broken payload", () => {
    const parsed = parseUptimeBody(snapshot("authority", { envelope: { kind: "ok", schema: "uptime/v9", polledEveryMs: null, body: body() } }));
    expect(parsed.kind).toBe("gap");
    if (parsed.kind === "gap") {
      expect(parsed.reason).toMatch(/Update the app/);
    }
  });
});

describe("uptimeBand", () => {
  it("expected on, reached the declared status: dormant (agreement)", () => {
    expect(uptimeBand({ expected: "on", expectStatus: 401, observedStatus: 401, error: null })).toBe("dormant");
  });

  it("expected on, unreachable: live (the door itself is shut)", () => {
    expect(uptimeBand({ expected: "on", expectStatus: 401, observedStatus: null, error: "timed out" })).toBe("live");
  });

  it("expected on, wrong status: near (the door is open, but not as declared)", () => {
    expect(uptimeBand({ expected: "on", expectStatus: 401, observedStatus: 500, error: null })).toBe("near");
  });

  it("expected off, unreachable: dormant (agreement — quiet, not a status word)", () => {
    expect(uptimeBand({ expected: "off", expectStatus: 200, observedStatus: null, error: "connection refused" })).toBe("dormant");
  });

  it("expected off, reachable: live (something is running that should not be)", () => {
    expect(uptimeBand({ expected: "off", expectStatus: 200, observedStatus: 200, error: null })).toBe("live");
  });
});

describe("uptimeCollapsedHeadline / uptimeGlyph", () => {
  it("names the service and the exact observation, distinctly per failure shape", () => {
    expect(uptimeCollapsedHeadline("authority", { expected: "on", expectStatus: 401, observedStatus: 401, error: null })).toBe(
      "authority · 401 as expected",
    );
    expect(
      uptimeCollapsedHeadline("runner", { expected: "on", expectStatus: 401, observedStatus: null, error: "dns error: NXDOMAIN" }),
    ).toBe("runner · unreachable — dns error: NXDOMAIN");
    expect(
      uptimeCollapsedHeadline("web", { expected: "on", expectStatus: 200, observedStatus: null, error: "connect timed out" }),
    ).toBe("web · unreachable — connect timed out");
    expect(uptimeCollapsedHeadline("authority", { expected: "on", expectStatus: 401, observedStatus: 500, error: null })).toBe(
      "authority · unexpected status 500 (wanted 401)",
    );
    expect(uptimeCollapsedHeadline("runner", { expected: "off", expectStatus: 401, observedStatus: null, error: "connection refused" })).toBe(
      "runner · off, as expected",
    );
    expect(uptimeCollapsedHeadline("runner", { expected: "off", expectStatus: 401, observedStatus: 401, error: null })).toBe(
      "runner · reachable — expected off",
    );
  });

  it("names the divergent service in the glyph label", () => {
    const glyph = uptimeGlyph("runner", { expected: "on", expectStatus: 401, observedStatus: null, error: "timed out" });
    expect(glyph).toEqual({ kind: "icon", name: "siren", label: "runner divergent" });
  });
});

describe("uptimeSubjects", () => {
  it("is the never-polled sentinel when nothing has been read", () => {
    expect(uptimeSubjects(inputs({ paneReads: {} }))).toEqual([NEVER_POLLED_SUBJECT]);
  });

  it("is every service key this source has ever written, sorted", () => {
    const rows = read([snapshot("web"), snapshot("authority"), snapshot("runner")]);
    expect(uptimeSubjects(inputs({ paneReads: { [SOURCE]: rows } }))).toEqual(["authority", "runner", "web"]);
  });
});

describe("uptimeAnswer", () => {
  it("is a gap, never an unbound question, when nothing has polled yet", () => {
    const answer = uptimeAnswer(NEVER_POLLED_SUBJECT, inputs({ paneReads: {} }));
    expect(answer.answerState).toBe("bound-but-unacquired");
    expect(answer.band).toBe("dormant");
    expect(answer.collapsedHeadline).toBe("No answer yet");
  });

  it("answers dormant agreement for a healthy expected-on service", () => {
    const answer = uptimeAnswer("authority", inputs());
    expect(answer.answerState).toBe("answered");
    expect(answer.band).toBe("dormant");
    expect(answer.collapsedHeadline).toBe("authority · 401 as expected");
  });

  /** ADR-0017 decision 4's own worked example — the sweeper's exclusion
   * protects this reading for a *live* deliberately-off service (this pane
   * never sees the sweeper itself, but `runner` taken down for a rebuild is
   * the same shape). */
  it("answers dormant agreement, never a status word, for an expected-off service correctly unreachable", () => {
    const rows = read([snapshot("runner", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ expected: "off", expectStatus: 401, observedStatus: null, error: "connection refused" }) } })]);
    const answer = uptimeAnswer("runner", inputs({ paneReads: { [SOURCE]: rows } }));
    expect(answer.band).toBe("dormant");
    expect(answer.collapsedHeadline).toBe("runner · off, as expected");
  });

  it("answers live with a distinct, visible reading for an unreachable host", () => {
    const rows = read([snapshot("authority", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ observedStatus: null, error: "connection refused" }) } })]);
    const answer = uptimeAnswer("authority", inputs({ paneReads: { [SOURCE]: rows } }));
    expect(answer.band).toBe("live");
    expect(answer.collapsedHeadline).toBe("authority · unreachable — connection refused");
  });

  it("answers live with a distinct, visible reading for a DNS failure — never the same wording as a timeout", () => {
    const dnsRow = read([snapshot("web", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ observedStatus: null, error: "dns error: NXDOMAIN" }) } })]);
    const timeoutRow = read([snapshot("web", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ observedStatus: null, error: "connect timed out" }) } })]);
    const dnsAnswer = uptimeAnswer("web", inputs({ paneReads: { [SOURCE]: dnsRow } }));
    const timeoutAnswer = uptimeAnswer("web", inputs({ paneReads: { [SOURCE]: timeoutRow } }));
    expect(dnsAnswer.collapsedHeadline).not.toBe(timeoutAnswer.collapsedHeadline);
    expect(dnsAnswer.collapsedHeadline).toMatch(/NXDOMAIN/);
    expect(timeoutAnswer.collapsedHeadline).toMatch(/timed out/);
  });

  it("answers live for the fault the other way — something reachable that should be off", () => {
    const rows = read([snapshot("runner", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ expected: "off", expectStatus: 401, observedStatus: 401, error: null }) } })]);
    const answer = uptimeAnswer("runner", inputs({ paneReads: { [SOURCE]: rows } }));
    expect(answer.band).toBe("live");
    expect(answer.collapsedHeadline).toBe("runner · reachable — expected off");
  });

  it("answers with a gap, not a healthy reading, for a malformed payload", () => {
    const rows = read([snapshot("authority", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: "not json" } })]);
    const answer = uptimeAnswer("authority", inputs({ paneReads: { [SOURCE]: rows } }));
    expect(answer.band).toBe("dormant");
    expect(answer.answerState).toBe("bound-but-unacquired");
    expect(answer.collapsedHeadline).toBe("No answer yet");
  });

  it("escalates a stale dormant reading rather than trusting a poller that may itself be dead", () => {
    const rows = read([snapshot("authority", { freshness: { kind: "age", ageMs: 4 * 60 * 60 * 1000, declaredCadenceMs: 3_600_000 } })]);
    const answer = uptimeAnswer("authority", inputs({ paneReads: { [SOURCE]: rows } }));
    expect(answer.band).toBe("imminent");
    expect(answer.collapsedHeadline).toMatch(/answer may be stale/);
  });

  it("does not escalate an already-visible divergent reading merely for being stale on top of it", () => {
    const rows = read([snapshot("authority", { envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ observedStatus: null, error: "connection refused" }) }, freshness: { kind: "age", ageMs: 4 * 60 * 60 * 1000, declaredCadenceMs: 3_600_000 } })]);
    const answer = uptimeAnswer("authority", inputs({ paneReads: { [SOURCE]: rows } }));
    expect(answer.band).toBe("live");
    expect(answer.collapsedHeadline).toMatch(/unreachable/);
  });
});

describe("uptimeView / uptimeGapReason", () => {
  it("returns null / the reason together, never disagreeing", () => {
    const emptyInputs = inputs({ paneReads: {} });
    expect(uptimeView("authority", emptyInputs)).toBeNull();
    expect(uptimeGapReason("authority", emptyInputs)).toMatch(/No answer|fetched yet/i);
  });
});

describe("isStaleFreshness / staleness", () => {
  it("never reads unknown as fresh, against this pane's own threshold", () => {
    const unknown: FreshnessDTO = { kind: "unknown" };
    expect(isStaleFreshness(unknown, STALE_AFTER_MS)).toBe(true);
  });

  it("bands stale at ~3h against the hourly cadence", () => {
    const freshRead = read([snapshot("authority", { freshness: { kind: "age", ageMs: 2 * 60 * 60 * 1000, declaredCadenceMs: 3_600_000 } })]);
    const staleRead = read([snapshot("authority", { freshness: { kind: "age", ageMs: 4 * 60 * 60 * 1000, declaredCadenceMs: 3_600_000 } })]);

    const freshView = uptimeView("authority", inputs({ paneReads: { [SOURCE]: freshRead } }));
    const staleView = uptimeView("authority", inputs({ paneReads: { [SOURCE]: staleRead } }));
    expect(freshView?.stale).toBe(false);
    expect(staleView?.stale).toBe(true);
  });
});
