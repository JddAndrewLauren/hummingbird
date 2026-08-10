import { describe, expect, it } from "vitest";
import { canRefresh } from "./refresh-gate";

// Issue #194: the header refresh control used to gate on the calendar
// alone (`calendar.connected && !calendar.needsReconnect`), which hid the
// button entirely on a task-only device — the default, and the state the
// owned-stack path actually cares about. The gate is now the union of what
// is actually refreshable: a task token, a healthy calendar connection, or
// both. Extracted as pure decision logic (this repo's own pattern —
// `status-label.ts`, `task/token-ui.ts`) so the matrix is provable without a
// DOM environment.

describe("canRefresh", () => {
  it("is false while the core is not ready, whatever else is true", () => {
    expect(canRefresh("loading", true, true, false)).toBe(false);
    expect(canRefresh("error", true, true, false)).toBe(false);
  });

  it("is true with a task token and no calendar connection", () => {
    expect(canRefresh("ready", true, false, false)).toBe(true);
  });

  it("is true with a healthy calendar connection and no task token", () => {
    expect(canRefresh("ready", false, true, false)).toBe(true);
  });

  it("is true with both a task token and a healthy calendar connection", () => {
    expect(canRefresh("ready", true, true, false)).toBe(true);
  });

  it("is false with neither a task token nor a calendar connection — no usable refresh, no button", () => {
    expect(canRefresh("ready", false, false, false)).toBe(false);
  });

  it("a calendar connection that needs reconnecting does not count as usable on its own", () => {
    expect(canRefresh("ready", false, true, true)).toBe(false);
  });

  it("a task token still makes it refreshable even while the calendar needs reconnecting", () => {
    expect(canRefresh("ready", true, true, true)).toBe(true);
  });
});
