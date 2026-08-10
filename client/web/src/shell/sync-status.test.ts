import { describe, expect, it } from "vitest";
import type { TaskSyncOutcome } from "../store/store";
import {
  deadLetterHeading,
  isInformativeSyncOutcome,
  syncOutcomeClass,
  syncStatusLabel,
  syncStatusTone,
  syncStatusToneWord,
} from "./sync-status";

function outcome(kind: TaskSyncOutcome["kind"]): TaskSyncOutcome {
  return { kind, retryAfterMs: null, activeItemCount: null, wasFullSweep: null, deadLettered: null };
}

describe("syncStatusLabel", () => {
  it("says Offline when offline, regardless of any recorded outcome", () => {
    expect(
      syncStatusLabel({
        online: false,
        lastSyncOutcome: outcome("completed"),
        lastSyncAtMs: 0,
        queueDepth: 0,
        nowMs: 1_000,
      }),
    ).toBe("Offline");
  });

  it("appends the queued count while offline, if any", () => {
    expect(
      syncStatusLabel({
        online: false,
        lastSyncOutcome: null,
        lastSyncAtMs: null,
        queueDepth: 2,
        nowMs: 1_000,
      }),
    ).toBe("Offline · 2 queued");
  });

  it.each<TaskSyncOutcome["kind"]>(["held", "credential_needed", "no_credential"])(
    "says Held when the last outcome was %s",
    (kind) => {
      expect(
        syncStatusLabel({
          online: true,
          lastSyncOutcome: outcome(kind),
          lastSyncAtMs: 1_000,
          queueDepth: 0,
          nowMs: 2_000,
        }),
      ).toBe("Held — device token needed");
    },
  );

  it("says Not yet synced before the first cycle this session", () => {
    expect(
      syncStatusLabel({
        online: true,
        lastSyncOutcome: null,
        lastSyncAtMs: null,
        queueDepth: 0,
        nowMs: 1_000,
      }),
    ).toBe("Not yet synced");
  });

  it("says Synced with an as-of age after a completed cycle", () => {
    expect(
      syncStatusLabel({
        online: true,
        lastSyncOutcome: outcome("completed"),
        lastSyncAtMs: 0,
        queueDepth: 0,
        nowMs: 42 * 60_000,
      }),
    ).toBe("Synced — as of 42m ago");
  });

  it.each<TaskSyncOutcome["kind"]>(["pull_failed", "persist_failed", "blocked"])(
    "says Stale, never Synced, after a %s outcome — honesty over reassurance",
    (kind) => {
      expect(
        syncStatusLabel({
          online: true,
          lastSyncOutcome: outcome(kind),
          lastSyncAtMs: 0,
          queueDepth: 0,
          nowMs: 5 * 60_000,
        }),
      ).toBe("Stale — as of 5m ago");
    },
  );

  it("appends the queued count once there is something queued", () => {
    expect(
      syncStatusLabel({
        online: true,
        lastSyncOutcome: outcome("completed"),
        lastSyncAtMs: 0,
        queueDepth: 1,
        nowMs: 0,
      }),
    ).toBe("Synced — as of just now · 1 queued");
  });

  it("never shows a zero-queued suffix — a 0 queued pill is decoration", () => {
    expect(
      syncStatusLabel({
        online: true,
        lastSyncOutcome: outcome("completed"),
        lastSyncAtMs: 0,
        queueDepth: 0,
        nowMs: 0,
      }),
    ).toBe("Synced — as of just now");
  });

  it("formats age in hours once past 60 minutes", () => {
    expect(
      syncStatusLabel({
        online: true,
        lastSyncOutcome: outcome("completed"),
        lastSyncAtMs: 0,
        queueDepth: 0,
        nowMs: 3 * 60 * 60_000,
      }),
    ).toBe("Synced — as of 3h ago");
  });

  it("formats age in days once past 24 hours", () => {
    expect(
      syncStatusLabel({
        online: true,
        lastSyncOutcome: outcome("completed"),
        lastSyncAtMs: 0,
        queueDepth: 0,
        nowMs: 2 * 24 * 60 * 60_000,
      }),
    ).toBe("Synced — as of 2d ago");
  });

  it("never reports a negative age from a clock that samples before the recorded sweep", () => {
    expect(
      syncStatusLabel({
        online: true,
        lastSyncOutcome: outcome("completed"),
        lastSyncAtMs: 10_000,
        queueDepth: 0,
        nowMs: 9_000,
      }),
    ).toBe("Synced — as of just now");
  });
});

describe("syncStatusTone", () => {
  it("is neutral while offline", () => {
    expect(
      syncStatusTone({ online: false, lastSyncOutcome: null, lastSyncAtMs: null, queueDepth: 0, nowMs: 0 }),
    ).toBe("neutral");
  });

  it("is warn while held", () => {
    expect(
      syncStatusTone({
        online: true,
        lastSyncOutcome: outcome("held"),
        lastSyncAtMs: 0,
        queueDepth: 0,
        nowMs: 0,
      }),
    ).toBe("warn");
  });

  it("is neutral before the first cycle this session", () => {
    expect(
      syncStatusTone({ online: true, lastSyncOutcome: null, lastSyncAtMs: null, queueDepth: 0, nowMs: 0 }),
    ).toBe("neutral");
  });

  it("is danger after a failed cycle", () => {
    expect(
      syncStatusTone({
        online: true,
        lastSyncOutcome: outcome("pull_failed"),
        lastSyncAtMs: 0,
        queueDepth: 0,
        nowMs: 0,
      }),
    ).toBe("danger");
  });

  it("is success after a completed cycle", () => {
    expect(
      syncStatusTone({
        online: true,
        lastSyncOutcome: outcome("completed"),
        lastSyncAtMs: 0,
        queueDepth: 0,
        nowMs: 0,
      }),
    ).toBe("success");
  });
});

describe("syncStatusToneWord", () => {
  // Round-2 review of PR #181: a fixed tone→word record said "not syncing"
  // next to a label saying "Offline". The word now branches with the label,
  // so each case is pinned against the label it renders beside.
  it("says offline while offline — matching the label's own word", () => {
    const input = {
      online: false,
      lastSyncOutcome: null,
      lastSyncAtMs: null,
      queueDepth: 0,
      nowMs: 0,
    };
    expect(syncStatusToneWord(input)).toBe("offline");
    expect(syncStatusLabel(input)).toBe("Offline");
  });

  it("says held while held", () => {
    const input = {
      online: true,
      lastSyncOutcome: outcome("held"),
      lastSyncAtMs: 0,
      queueDepth: 0,
      nowMs: 0,
    };
    expect(syncStatusToneWord(input)).toBe("held");
    expect(syncStatusLabel(input)).toContain("Held");
  });

  it("says not synced before the first cycle this session", () => {
    const input = {
      online: true,
      lastSyncOutcome: null,
      lastSyncAtMs: null,
      queueDepth: 0,
      nowMs: 0,
    };
    expect(syncStatusToneWord(input)).toBe("not synced");
    expect(syncStatusLabel(input)).toBe("Not yet synced");
  });

  it("says stale after a failed cycle", () => {
    const input = {
      online: true,
      lastSyncOutcome: outcome("pull_failed"),
      lastSyncAtMs: 0,
      queueDepth: 0,
      nowMs: 0,
    };
    expect(syncStatusToneWord(input)).toBe("stale");
    expect(syncStatusLabel(input)).toContain("Stale");
  });

  it("says synced after a completed cycle", () => {
    const input = {
      online: true,
      lastSyncOutcome: outcome("completed"),
      lastSyncAtMs: 0,
      queueDepth: 0,
      nowMs: 0,
    };
    expect(syncStatusToneWord(input)).toBe("synced");
    expect(syncStatusLabel(input)).toContain("Synced");
  });
});

// Post-batch review of PR #185: `"skipped"` and `"busy"` were in neither
// `HELD_KINDS` nor `FAILED_KINDS` and fell through to the SUCCESS branch, so
// a backed-off tick during a server outage (`Core::run` -> `Skipped`,
// "nothing was attempted at all", cycle.rs:166-168) read as
// "Synced — as of just now" and re-greened the badge every 60 seconds for
// the whole outage. This file never mentioned either kind, which is how a
// 272-line test file missed it.
describe("the cycles that never ran — skipped and busy", () => {
  const NOT_RUN: TaskSyncOutcome["kind"][] = ["skipped", "busy"];

  it.each(NOT_RUN)("classifies %s as not-run, i.e. carrying no staleness information", (kind) => {
    expect(syncOutcomeClass(kind)).toBe("not-run");
    expect(isInformativeSyncOutcome(kind)).toBe(false);
  });

  it.each<TaskSyncOutcome["kind"]>([
    "held",
    "credential_needed",
    "no_credential",
    "pull_failed",
    "persist_failed",
    "blocked",
    "completed",
  ])("treats %s as informative — every other kind says something about staleness", (kind) => {
    expect(isInformativeSyncOutcome(kind)).toBe(true);
  });

  it.each(NOT_RUN)("never lets a %s outcome read as Synced", (kind) => {
    const input = {
      online: true,
      lastSyncOutcome: outcome(kind),
      lastSyncAtMs: 0,
      queueDepth: 0,
      nowMs: 60_000,
    };
    expect(syncStatusLabel(input)).toBe("Stale — as of 1m ago");
    expect(syncStatusTone(input)).toBe("danger");
    expect(syncStatusToneWord(input)).toBe("stale");
  });

  it.each(NOT_RUN)("still says Not yet synced for %s before any cycle has landed", (kind) => {
    const input = {
      online: true,
      lastSyncOutcome: outcome(kind),
      lastSyncAtMs: null,
      queueDepth: 0,
      nowMs: 60_000,
    };
    expect(syncStatusLabel(input)).toBe("Not yet synced");
    expect(syncStatusTone(input)).toBe("neutral");
    expect(syncStatusToneWord(input)).toBe("not synced");
  });

  it.each(NOT_RUN)("still says Offline for %s while offline", (kind) => {
    const input = {
      online: false,
      lastSyncOutcome: outcome(kind),
      lastSyncAtMs: 0,
      queueDepth: 0,
      nowMs: 0,
    };
    expect(syncStatusLabel(input)).toBe("Offline");
    expect(syncStatusTone(input)).toBe("neutral");
    expect(syncStatusToneWord(input)).toBe("offline");
  });
});

describe("deadLetterHeading", () => {
  it("uses the singular for exactly one entry", () => {
    expect(deadLetterHeading(1)).toBe("1 edit didn't apply");
  });

  it.each([0, 2, 5])("pluralises for any count other than one (%i)", (count) => {
    expect(deadLetterHeading(count)).toBe(`${count} edits didn't apply`);
  });
});
