import { describe, expect, it, vi } from "vitest";
import {
  createSyncCadence,
  mergePendingSyncTrigger,
  shouldRunTimerTick,
  toCoreTrigger,
} from "./sync-cadence";

describe("shouldRunTimerTick", () => {
  it("runs when visible and online", () => {
    expect(shouldRunTimerTick(false, true)).toBe(true);
  });

  it("is paused while the document is hidden, even if online", () => {
    expect(shouldRunTimerTick(true, true)).toBe(false);
  });

  it("is paused while offline, even if visible", () => {
    expect(shouldRunTimerTick(false, false)).toBe(false);
  });

  it("is paused when both hidden and offline", () => {
    expect(shouldRunTimerTick(true, false)).toBe(false);
  });
});

describe("toCoreTrigger", () => {
  it("maps every user-facing trigger to \"user\" — ADR-0007 resets backoff on a user trigger", () => {
    expect(toCoreTrigger("open")).toBe("user");
    expect(toCoreTrigger("reconnect")).toBe("user");
    expect(toCoreTrigger("focus")).toBe("user");
    expect(toCoreTrigger("manual")).toBe("user");
  });

  it('maps the unattended timer to "timer"', () => {
    expect(toCoreTrigger("timer")).toBe("timer");
  });
});

describe("mergePendingSyncTrigger", () => {
  // Issue #184 round 2: a pending "open" carries ADR-0008/#193's
  // app-open full-sweep backstop — dropping it is not merely a delay, it is
  // lost for the whole SharedWorker lifetime (`openTrigger` only fires
  // once). It must survive being merged against every other trigger,
  // arriving either before or after it.
  it("keeps a pending \"open\" against every later trigger", () => {
    expect(mergePendingSyncTrigger("open", "focus")).toBe("open");
    expect(mergePendingSyncTrigger("open", "reconnect")).toBe("open");
    expect(mergePendingSyncTrigger("open", "manual")).toBe("open");
    expect(mergePendingSyncTrigger("open", "timer")).toBe("open");
    expect(mergePendingSyncTrigger("open", "open")).toBe("open");
  });

  it("an incoming \"open\" always wins, regardless of what is pending", () => {
    expect(mergePendingSyncTrigger("focus", "open")).toBe("open");
    expect(mergePendingSyncTrigger("reconnect", "open")).toBe("open");
    expect(mergePendingSyncTrigger("manual", "open")).toBe("open");
    expect(mergePendingSyncTrigger("timer", "open")).toBe("open");
  });

  // Issue #194: a pending user-facing trigger (which resets backoff via
  // `toCoreTrigger`'s "user" spelling) must not be silently demoted to
  // "timer" (which does not) by a later, unattended timer tick.
  it("keeps a pending user-facing trigger against a later \"timer\"", () => {
    expect(mergePendingSyncTrigger("focus", "timer")).toBe("focus");
    expect(mergePendingSyncTrigger("reconnect", "timer")).toBe("reconnect");
    expect(mergePendingSyncTrigger("manual", "timer")).toBe("manual");
  });

  it("a later user-facing trigger replaces a pending \"timer\"", () => {
    expect(mergePendingSyncTrigger("timer", "focus")).toBe("focus");
    expect(mergePendingSyncTrigger("timer", "reconnect")).toBe("reconnect");
    expect(mergePendingSyncTrigger("timer", "manual")).toBe("manual");
  });

  // Among the three user-facing triggers there is no caller-visible
  // behavioral difference downstream of the guard (all map to "user", none
  // forces a full sweep), so ties fall back to "most recent wins".
  it("a later user-facing trigger replaces a different pending user-facing trigger", () => {
    expect(mergePendingSyncTrigger("focus", "manual")).toBe("manual");
    expect(mergePendingSyncTrigger("manual", "reconnect")).toBe("reconnect");
    expect(mergePendingSyncTrigger("reconnect", "focus")).toBe("focus");
  });

  it("a later \"timer\" replaces a pending \"timer\"", () => {
    expect(mergePendingSyncTrigger("timer", "timer")).toBe("timer");
  });
});

describe("createSyncCadence", () => {
  // #193: `run` receives the real `SyncCadenceTrigger` — not the
  // "user"/"timer" spelling `toCoreTrigger` maps it to — so the one caller
  // (`core.worker.ts`) can tell an open cycle apart from a focus or
  // reconnect cycle and decide `forceFullSweep` itself.
  it("onOpen fires exactly one cycle with the \"open\" trigger", () => {
    const run = vi.fn();
    createSyncCadence(run).onOpen();
    expect(run).toHaveBeenCalledTimes(1);
    expect(run).toHaveBeenCalledWith("open");
  });

  it("onReconnect fires exactly one cycle with the \"reconnect\" trigger", () => {
    const run = vi.fn();
    createSyncCadence(run).onReconnect();
    expect(run).toHaveBeenCalledTimes(1);
    expect(run).toHaveBeenCalledWith("reconnect");
  });

  it("onFocus fires exactly one cycle with the \"focus\" trigger", () => {
    const run = vi.fn();
    createSyncCadence(run).onFocus();
    expect(run).toHaveBeenCalledTimes(1);
    expect(run).toHaveBeenCalledWith("focus");
  });

  // Issue #194: manual refresh is ADR-0007's "same cycle, user-invoked; no
  // special path" — its own trigger spelling so `core.worker.ts` can tell it
  // apart from a focus/open/reconnect cycle if it ever needs to, same as
  // #193 did for "open".
  it("onManual fires exactly one cycle with the \"manual\" trigger", () => {
    const run = vi.fn();
    createSyncCadence(run).onManual();
    expect(run).toHaveBeenCalledTimes(1);
    expect(run).toHaveBeenCalledWith("manual");
  });

  it("two onFocus calls fire exactly two cycles — no de-dupe hides a real repeated gesture", () => {
    const run = vi.fn();
    const cadence = createSyncCadence(run);
    cadence.onFocus();
    cadence.onFocus();
    expect(run).toHaveBeenCalledTimes(2);
  });

  it("onTimerTick fires a timer-triggered cycle when visible and online", () => {
    const run = vi.fn();
    createSyncCadence(run).onTimerTick(false, true);
    expect(run).toHaveBeenCalledTimes(1);
    expect(run).toHaveBeenCalledWith("timer");
  });

  it("onTimerTick fires nothing while hidden — the sync timer is paused, proven", () => {
    const run = vi.fn();
    createSyncCadence(run).onTimerTick(true, true);
    expect(run).not.toHaveBeenCalled();
  });

  it("onTimerTick fires nothing while offline", () => {
    const run = vi.fn();
    createSyncCadence(run).onTimerTick(false, false);
    expect(run).not.toHaveBeenCalled();
  });
});
