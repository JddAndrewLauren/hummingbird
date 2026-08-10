import { describe, expect, it } from "vitest";
import type { TaskSyncOutcome } from "../store/store";
import { deadLetterHeading, syncStatusLabel, syncStatusTone, syncStatusToneWord } from "./sync-status";

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

describe("deadLetterHeading", () => {
  it("uses the singular for exactly one entry", () => {
    expect(deadLetterHeading(1)).toBe("1 edit didn't apply");
  });

  it.each([0, 2, 5])("pluralises for any count other than one (%i)", (count) => {
    expect(deadLetterHeading(count)).toBe(`${count} edits didn't apply`);
  });
});
