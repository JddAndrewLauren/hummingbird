import { describe, expect, it } from "vitest";
import type { FreshnessDTO, PaneReadDTO, PaneSnapshotDTO } from "../../store/protocol";
import { EMPTY_QUESTION_SYNC, type QuestionInputs } from "../questions/contract";
import {
  NEVER_POLLED_SUBJECT,
  OVERDUE_MULTIPLIER,
  SOURCE,
  STALE_AFTER_MS,
  githubAnswer,
  githubBand,
  githubCollapsedHeadline,
  githubGapReason,
  githubGlyph,
  githubSubjects,
  githubView,
  isStaleFreshness,
  parseWorkflowBody,
} from "./github";

// #314's own pane: the first Status question whose subject list comes from
// which snapshot keys exist, not a binding or a fixed sentinel. Everything
// here is pure: the clock arrives on `QuestionInputs`, exactly like
// `kimi.test.ts`.

const NOW = Date.parse("2026-08-12T16:00:00Z");
const FILE_NAME = "gmail-poll.yml";

interface BodyOverrides {
  displayName?: string;
  declaredCadenceMs?: number | null;
  lastRunConclusion?: string | null;
  lastRunEvent?: string | null;
  lastRunAtMs?: number | null;
  lastScheduledSuccessAtMs?: number | null;
}

// `??` cannot be used field-by-field here: an override explicitly passing
// `null` (the auto-disable shape) must win over the default, and `null ??
// default` reads as "not provided" — so each field falls back only when the
// key is genuinely absent from `overrides`.
function body(overrides: BodyOverrides = {}): string {
  return JSON.stringify({
    display_name: "displayName" in overrides ? overrides.displayName : "gmail-poll",
    declared_cadence_ms: "declaredCadenceMs" in overrides ? overrides.declaredCadenceMs : 15 * 60 * 1000,
    last_run_conclusion: "lastRunConclusion" in overrides ? overrides.lastRunConclusion : "success",
    last_run_event: "lastRunEvent" in overrides ? overrides.lastRunEvent : "schedule",
    last_run_at_ms: "lastRunAtMs" in overrides ? overrides.lastRunAtMs : NOW - 60_000,
    last_scheduled_success_at_ms:
      "lastScheduledSuccessAtMs" in overrides ? overrides.lastScheduledSuccessAtMs : NOW - 60_000,
  });
}

function snapshot(overrides: Partial<PaneSnapshotDTO> = {}): PaneSnapshotDTO {
  return {
    key: FILE_NAME,
    fetchedAtMs: NOW - 60_000,
    envelope: { kind: "ok", schema: SOURCE, polledEveryMs: 86_400_000, body: body() },
    freshness: { kind: "age", ageMs: 60_000, declaredCadenceMs: 86_400_000 },
    ...overrides,
  };
}

function read(snapshots: PaneSnapshotDTO[] = [snapshot()]): PaneReadDTO {
  return { source: SOURCE, snapshots, liveAlerts: [] };
}

function inputs(overrides: Partial<QuestionInputs> = {}): QuestionInputs {
  return {
    sync: EMPTY_QUESTION_SYNC,
    bindings: [],
    paneReads: { [SOURCE]: read() },
    calendarReads: {},
    calendarConnected: false,
    items: [],
    nowMs: NOW,
    ...overrides,
  };
}

describe("parseWorkflowBody", () => {
  it("reads a well-formed body", () => {
    const parsed = parseWorkflowBody(snapshot());
    expect(parsed).toEqual({
      kind: "ok",
      body: {
        displayName: "gmail-poll",
        declaredCadenceMs: 15 * 60 * 1000,
        lastRunConclusion: "success",
        lastRunEvent: "schedule",
        lastRunAtMs: NOW - 60_000,
        lastScheduledSuccessAtMs: NOW - 60_000,
      },
    });
  });

  it("reads the auto-disable shape — every field null but the name", () => {
    const parsed = parseWorkflowBody(
      snapshot({
        envelope: {
          kind: "ok",
          schema: SOURCE,
          polledEveryMs: null,
          body: body({
            declaredCadenceMs: null,
            lastRunConclusion: null,
            lastRunEvent: null,
            lastRunAtMs: null,
            lastScheduledSuccessAtMs: null,
          }),
        },
      }),
    );
    expect(parsed.kind).toBe("ok");
    if (parsed.kind === "ok") {
      expect(parsed.body.lastRunAtMs).toBeNull();
      expect(parsed.body.lastScheduledSuccessAtMs).toBeNull();
    }
  });

  it("says why it could not, rather than answering emptily", () => {
    const cases: [PaneSnapshotDTO | undefined, RegExp][] = [
      [undefined, /No answer has been fetched/],
      [snapshot({ envelope: { kind: "malformed", reason: "`body` is missing" } }), /couldn't be read/],
      [snapshot({ envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: "{" } }), /isn't JSON/],
      [snapshot({ envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: "[]" } }), /isn't an object/],
      [
        snapshot({
          envelope: {
            kind: "ok",
            schema: SOURCE,
            polledEveryMs: null,
            body: JSON.stringify({ display_name: 12 }),
          },
        }),
        /fields can't be read/,
      ],
    ];
    for (const [row, pattern] of cases) {
      const parsed = parseWorkflowBody(row);
      expect(parsed.kind).toBe("gap");
      if (parsed.kind === "gap") {
        expect(parsed.reason).toMatch(pattern);
      }
    }
  });

  it("reads an unrecognised schema as this device being behind, not as a broken payload", () => {
    const parsed = parseWorkflowBody(
      snapshot({ envelope: { kind: "ok", schema: "github-hummingbird/v9", polledEveryMs: null, body: body() } }),
    );
    expect(parsed.kind).toBe("gap");
    if (parsed.kind === "gap") {
      expect(parsed.reason).toMatch(/Update the app/);
    }
  });
});

describe("githubBand", () => {
  const healthy = {
    displayName: "gmail-poll",
    declaredCadenceMs: 15 * 60 * 1000,
    lastRunConclusion: "success",
    lastRunEvent: "schedule",
    lastRunAtMs: NOW - 60_000,
    lastScheduledSuccessAtMs: NOW - 60_000,
  };

  it("bands a healthy, on-cadence workflow dormant", () => {
    expect(githubBand(healthy, NOW)).toBe("dormant");
  });

  it("bands the never-run (auto-disable) shape live — the most severe reading", () => {
    expect(githubBand({ ...healthy, lastRunAtMs: null, lastScheduledSuccessAtMs: null }, NOW)).toBe("live");
  });

  it("bands a manual-only run imminent — a green manual run never masks a dead cron", () => {
    expect(
      githubBand(
        { ...healthy, lastRunEvent: "workflow_dispatch", lastScheduledSuccessAtMs: null },
        NOW,
      ),
    ).toBe("imminent");
  });

  it("bands overdue against the declared cadence imminent, even with a stored success", () => {
    const cadence = 15 * 60 * 1000;
    const justUnderThreshold = NOW - cadence * OVERDUE_MULTIPLIER + 1;
    const justOverThreshold = NOW - cadence * OVERDUE_MULTIPLIER - 1;
    expect(
      githubBand({ ...healthy, declaredCadenceMs: cadence, lastScheduledSuccessAtMs: justUnderThreshold }, NOW),
    ).toBe("dormant");
    expect(
      githubBand({ ...healthy, declaredCadenceMs: cadence, lastScheduledSuccessAtMs: justOverThreshold }, NOW),
    ).toBe("imminent");
  });

  it("bands a single recent failure near — less urgent than a stopped cron", () => {
    expect(githubBand({ ...healthy, lastRunConclusion: "failure" }, NOW)).toBe("near");
  });

  it("a stopped cron sorts above a single failure — the brief's own acceptance line", () => {
    const bandOrder = ["live", "imminent", "near", "distant", "dormant"];
    const stoppedBand = githubBand({ ...healthy, lastRunAtMs: null, lastScheduledSuccessAtMs: null }, NOW);
    const failedBand = githubBand({ ...healthy, lastRunConclusion: "failure" }, NOW);
    expect(bandOrder.indexOf(stoppedBand)).toBeLessThan(bandOrder.indexOf(failedBand));
  });

  it("does not judge overdue-ness when the cadence is unrecognised, and never reports that as healthy", () => {
    const band = githubBand(
      { ...healthy, declaredCadenceMs: null, lastScheduledSuccessAtMs: NOW - 999_999_999 },
      NOW,
    );
    expect(band).toBe("distant");
    // The regression this blocker is about: an unreadable cadence must
    // never fall through to the same band a genuinely healthy, on-cadence
    // workflow gets. If this ever comes back `dormant`, a workflow GitHub
    // silently auto-disabled 60 days ago would read as healthy forever.
    expect(band).not.toBe("dormant");
  });

  it("still reports a genuine failure (near) ahead of an unreadable cadence (distant)", () => {
    expect(
      githubBand({ ...healthy, declaredCadenceMs: null, lastRunConclusion: "failure" }, NOW),
    ).toBe("near");
  });
});

describe("githubCollapsedHeadline", () => {
  const healthy = {
    displayName: "gmail-poll",
    declaredCadenceMs: 15 * 60 * 1000,
    lastRunConclusion: "success",
    lastRunEvent: "schedule",
    lastRunAtMs: NOW - 60_000,
    lastScheduledSuccessAtMs: NOW - 60_000,
  };

  it("names the workflow in every band", () => {
    expect(githubCollapsedHeadline(healthy, NOW)).toBe("gmail-poll · healthy");
    expect(githubCollapsedHeadline({ ...healthy, lastRunAtMs: null, lastScheduledSuccessAtMs: null }, NOW)).toBe(
      "gmail-poll · never run",
    );
    expect(githubCollapsedHeadline({ ...healthy, lastRunConclusion: "failure" }, NOW)).toBe(
      "gmail-poll · last run failed",
    );
  });

  it("says the cadence is unreadable rather than healthy, and names how long ago the last scheduled success was", () => {
    const twoHoursAgo = NOW - 2 * 60 * 60 * 1000;
    const headline = githubCollapsedHeadline(
      { ...healthy, declaredCadenceMs: null, lastScheduledSuccessAtMs: twoHoursAgo },
      NOW,
    );
    expect(headline).toBe("gmail-poll · cadence unreadable, last scheduled success 2h ago");
    expect(headline).not.toContain("healthy");
  });
});

describe("githubGlyph", () => {
  const healthy = {
    displayName: "gmail-poll",
    declaredCadenceMs: 15 * 60 * 1000,
    lastRunConclusion: "success",
    lastRunEvent: "schedule",
    lastRunAtMs: NOW - 60_000,
    lastScheduledSuccessAtMs: NOW - 60_000,
  };

  it("uses a distinct glyph for an unreadable cadence — never the healthy circle-check", () => {
    const glyph = githubGlyph({ ...healthy, declaredCadenceMs: null }, NOW);
    expect(glyph).toEqual({ kind: "icon", name: "help-circle", label: "gmail-poll cadence unreadable" });
    expect(glyph.kind === "icon" && glyph.name).not.toBe("circle-check");
  });
});

describe("githubSubjects", () => {
  it("returns the sentinel when nothing has been read yet", () => {
    expect(githubSubjects(inputs({ paneReads: {} }))).toEqual([NEVER_POLLED_SUBJECT]);
  });

  it("returns the sentinel when the source has been read but has no rows", () => {
    expect(githubSubjects(inputs({ paneReads: { [SOURCE]: read([]) } }))).toEqual([NEVER_POLLED_SUBJECT]);
  });

  it("returns one subject per snapshot key, sorted", () => {
    const rows = read([
      snapshot({ key: "graph-mail-poll.yml" }),
      snapshot({ key: "calendar-poll.yml" }),
    ]);
    expect(githubSubjects(inputs({ paneReads: { [SOURCE]: rows } }))).toEqual([
      "calendar-poll.yml",
      "graph-mail-poll.yml",
    ]);
  });
});

describe("githubAnswer", () => {
  it("answers with the workflow's own headline when a snapshot has landed", () => {
    const answer = githubAnswer(FILE_NAME, inputs());
    expect(answer.answerState).toBe("answered");
    expect(answer.band).toBe("dormant");
    expect(answer.withinBand).toBeNull();
    expect(answer.collapsedHeadline).toBe("gmail-poll · healthy");
  });

  it("is a gap, never an unbound question, for the never-polled sentinel", () => {
    const answer = githubAnswer(NEVER_POLLED_SUBJECT, inputs({ paneReads: {} }));
    expect(answer.answerState).toBe("bound-but-unacquired");
    expect(answer.band).toBe("dormant");
    expect(answer.collapsedHeadline).toBe("No answer yet");
  });

  it("bands live and shows a siren glyph once a workflow has never run at all", () => {
    const rows = read([snapshot({ envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ lastRunAtMs: null, lastScheduledSuccessAtMs: null, lastRunConclusion: null, lastRunEvent: null }) } })]);
    const answer = githubAnswer(FILE_NAME, inputs({ paneReads: { [SOURCE]: rows } }));
    expect(answer.band).toBe("live");
    expect(answer.icon?.[0]).toEqual({ kind: "icon", name: "siren", label: "gmail-poll never run" });
  });

  // Blocker 3 (#371 review round 1): the self-death tell — this pane's own
  // staleness must be visible on the *collapsed* row, since `dormant` and
  // `distant` panes never render `GithubPaneExpanded` (the only other place
  // `view.stale` was read). If `github-status.yml` itself stops running,
  // every row it ever wrote keeps reporting whatever it last saw; the only
  // way a reader learns anything is wrong is if staleness escalates the
  // band here.
  const staleFreshness: FreshnessDTO = { kind: "age", ageMs: STALE_AFTER_MS + 60_000, declaredCadenceMs: 86_400_000 };

  it("escalates a stale dormant (healthy) reading to imminent, and says so in the headline", () => {
    const rows = read([snapshot({ freshness: staleFreshness })]);
    const answer = githubAnswer(FILE_NAME, inputs({ paneReads: { [SOURCE]: rows } }));
    expect(answer.band).not.toBe("dormant");
    expect(answer.band).toBe("imminent");
    expect(answer.collapsedHeadline).not.toContain("healthy");
    expect(answer.collapsedHeadline).toContain("stale");
  });

  it("escalates a stale distant (cadence-unreadable) reading to imminent", () => {
    const rows = read([
      snapshot({
        freshness: staleFreshness,
        envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ declaredCadenceMs: null }) },
      }),
    ]);
    const answer = githubAnswer(FILE_NAME, inputs({ paneReads: { [SOURCE]: rows } }));
    expect(answer.band).toBe("imminent");
    expect(answer.collapsedHeadline).toContain("stale");
  });

  it("does not need a second escalation for a band that is already non-green when stale", () => {
    const rows = read([
      snapshot({
        freshness: staleFreshness,
        envelope: { kind: "ok", schema: SOURCE, polledEveryMs: null, body: body({ lastRunConclusion: "failure" }) },
      }),
    ]);
    const answer = githubAnswer(FILE_NAME, inputs({ paneReads: { [SOURCE]: rows } }));
    expect(answer.band).toBe("near");
    expect(answer.collapsedHeadline).toBe("gmail-poll · last run failed");
  });
});

describe("githubView / githubGapReason", () => {
  it("returns null / the reason together, never disagreeing", () => {
    const emptyInputs = inputs({ paneReads: {} });
    expect(githubView(FILE_NAME, emptyInputs)).toBeNull();
    expect(githubGapReason(FILE_NAME, emptyInputs)).toMatch(/No answer/i);
  });
});

describe("isStaleFreshness / staleness", () => {
  it("never reads unknown as fresh, against this pane's own threshold", () => {
    const unknown: FreshnessDTO = { kind: "unknown" };
    expect(isStaleFreshness(unknown, STALE_AFTER_MS)).toBe(true);
  });

  /** The module-level half of the `stale — age unknown` branch: with an
   * unageable freshness the escalated headline must say so in words rather
   * than reporting a number it does not have. */
  it("escalates an unageable freshness and says so without inventing an age", () => {
    const rows = read([snapshot({ freshness: { kind: "unknown" } })]);
    const answer = githubAnswer(FILE_NAME, inputs({ paneReads: { [SOURCE]: rows } }));
    expect(answer.band).toBe("imminent");
    expect(answer.collapsedHeadline).toContain("last heard an unknown time ago");
  });

  it("bands stale at ~30h against the daily cadence", () => {
    const freshRead = read([snapshot({ freshness: { kind: "age", ageMs: 29 * 60 * 60 * 1000, declaredCadenceMs: 86_400_000 } })]);
    const staleRead = read([snapshot({ freshness: { kind: "age", ageMs: 31 * 60 * 60 * 1000, declaredCadenceMs: 86_400_000 } })]);

    const freshView = githubView(FILE_NAME, inputs({ paneReads: { [SOURCE]: freshRead } }));
    const staleView = githubView(FILE_NAME, inputs({ paneReads: { [SOURCE]: staleRead } }));
    expect(freshView?.stale).toBe(false);
    expect(staleView?.stale).toBe(true);
  });
});
