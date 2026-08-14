import { describe, expect, it } from "vitest";
import type { QuestionInputs } from "../questions/contract";
import { REACHABILITY_GRACE_MS, reachabilityAnswer } from "./reachability";

const NOW = 1_700_000_000_000;

function inputs(
  sync: Partial<QuestionInputs["sync"]> = {},
  nowMs = NOW,
): QuestionInputs {
  return {
    sync: {
      latestOutcome: null,
      latestInformativeAtMs: null,
      lastSuccessfulAtMs: null,
      ...sync,
    },
    bindings: [],
    paneReads: {},
    calendarReads: {},
    calendarConnected: false,
    items: [],
    nowMs,
  };
}

function outcome(
  kind: NonNullable<QuestionInputs["sync"]["latestOutcome"]>["kind"],
  retryAfterMs: number | null = null,
) {
  return {
    kind,
    retryAfterMs,
    activeItemCount: null,
    wasFullSweep: null,
    deadLettered: null,
  };
}

describe("reachabilityAnswer", () => {
  it("keeps a fresh completed sync quiet and calls it synced", () => {
    const atMs = NOW - 60_000;
    const answer = reachabilityAnswer(inputs({
      latestOutcome: outcome("completed"),
      latestInformativeAtMs: atMs,
      lastSuccessfulAtMs: atMs,
    }));
    expect(answer).toMatchObject({ answerState: "answered", band: "dormant", collapsedHeadline: "Synced 1m ago" });
    expect(answer.withinBand).toBe(atMs + REACHABILITY_GRACE_MS);
  });

  it("calls an earlier success 'last synced' after one newer missed cycle", () => {
    const successAtMs = NOW - 2 * 60_000;
    const answer = reachabilityAnswer(inputs({
      latestOutcome: outcome("pull_failed"),
      latestInformativeAtMs: NOW - 60_000,
      lastSuccessfulAtMs: successAtMs,
    }));
    expect(answer).toMatchObject({ band: "dormant", collapsedHeadline: "Last synced 2m ago" });
  });

  it("keeps the maximum-backoff recovery window quiet without calling it synced", () => {
    const successAtMs = NOW - 5 * 60_000 - 30_000;
    const answer = reachabilityAnswer(inputs({
      latestOutcome: outcome("pull_failed", 5 * 60_000),
      latestInformativeAtMs: successAtMs + 60_000,
      lastSuccessfulAtMs: successAtMs,
    }));
    expect(answer).toMatchObject({ band: "dormant", collapsedHeadline: "Last synced 5m ago" });
  });

  it("includes the cadence-plus-backoff boundary in the quiet grace", () => {
    expect(reachabilityAnswer(inputs({ lastSuccessfulAtMs: NOW - REACHABILITY_GRACE_MS })).band).toBe("dormant");
  });

  it("escalates sustained failure after the grace ceiling", () => {
    const answer = reachabilityAnswer(inputs({
      latestOutcome: outcome("pull_failed"),
      latestInformativeAtMs: NOW - 5 * 60_000,
      lastSuccessfulAtMs: NOW - REACHABILITY_GRACE_MS - 1,
    }));
    expect(answer).toMatchObject({ answerState: "answered", band: "live", collapsedHeadline: "Last synced 6m ago" });
  });

  it("renders a never-synced gap", () => {
    expect(reachabilityAnswer(inputs())).toMatchObject({
      answerState: "bound-but-unacquired",
      band: "dormant",
      collapsedHeadline: "Never synced on this device.",
    });
  });

  it("clamps clock skew instead of producing a negative age", () => {
    expect(reachabilityAnswer(inputs({ lastSuccessfulAtMs: NOW + 60_000 })).collapsedHeadline).toBe("Last synced just now");
  });

  it.each(["skipped", "busy"] as const)("never calls a direct %s outcome success", (kind) => {
    const atMs = NOW - 60_000;
    expect(reachabilityAnswer(inputs({
      latestOutcome: outcome(kind),
      latestInformativeAtMs: atMs,
      lastSuccessfulAtMs: atMs,
    })).collapsedHeadline).toBe("Last synced 1m ago");
  });
});
