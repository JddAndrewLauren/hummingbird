import { describe, expect, it } from "vitest";
import { navAlarmColor } from "./nav-alarm";

describe("navAlarmColor", () => {
  it("wears no tint when nothing raises the nav", () => {
    expect(navAlarmColor(undefined)).toBeUndefined();
  });

  it("reads danger and warn off the board's own band table", () => {
    expect(navAlarmColor("live")).toBe("var(--status-danger-fg)");
    expect(navAlarmColor("imminent")).toBe("var(--status-danger-fg)");
    expect(navAlarmColor("near")).toBe("var(--status-warn-fg)");
    expect(navAlarmColor("distant")).toBe("var(--status-warn-fg)");
  });

  // The core never returns this — `alarm.rs` folds `dormant` away — but the
  // type permits it, and a nav that invented a third tint for it would be
  // painting "everything is fine" as a state worth noticing.
  it("wears no tint for a quiet band", () => {
    expect(navAlarmColor("dormant")).toBeUndefined();
  });
});
