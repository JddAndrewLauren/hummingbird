import { describe, expect, it } from "vitest";
import { formatEnteredAt, taskQueueStatusCopy, taskTokenUiState } from "./token-ui";

describe("taskTokenUiState", () => {
  it("is unset for a device with no stored token, regardless of any stale reconnect flag", () => {
    expect(taskTokenUiState(false, false)).toBe("unset");
    expect(taskTokenUiState(false, true)).toBe("unset");
  });

  it("is reprompt for a stored token the core has flagged as no longer working", () => {
    expect(taskTokenUiState(true, true)).toBe("reprompt");
  });

  it("is resting for a stored token with no reconnect flag", () => {
    expect(taskTokenUiState(true, false)).toBe("resting");
  });
});

describe("taskQueueStatusCopy", () => {
  it("distinguishes the unset state from a held queue", () => {
    const unset = taskQueueStatusCopy("unset");
    const reprompt = taskQueueStatusCopy("reprompt");
    expect(unset).not.toBe(reprompt);
    expect(unset.toLowerCase()).toContain("queued");
    expect(reprompt.toLowerCase()).toContain("held");
  });

  it("never claims the token expired — revocation is the real cause, not expiry", () => {
    expect(taskQueueStatusCopy("reprompt").toLowerCase()).not.toContain("expired");
  });

  it("describes the resting state as syncing normally", () => {
    expect(taskQueueStatusCopy("resting").toLowerCase()).toContain("sync");
  });
});

describe("formatEnteredAt", () => {
  it("renders a fixed instant as an ISO-ish readable string", () => {
    // 2026-01-01T00:00:00.000Z
    expect(formatEnteredAt(1_767_225_600_000)).toContain("2026");
  });
});
