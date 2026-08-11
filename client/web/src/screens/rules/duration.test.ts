import { describe, expect, it } from "vitest";
import { durationUnitsFor, formatDuration, isBelowAlarmInterval, parseDurationMs } from "./duration";

describe("parseDurationMs", () => {
  it("parses minutes, hours and days", () => {
    expect(parseDurationMs("10m")).toBe(10 * 60_000);
    expect(parseDurationMs("2h")).toBe(2 * 60 * 60_000);
    expect(parseDurationMs("3d")).toBe(3 * 24 * 60 * 60_000);
  });

  it("rejects a zero or negative amount", () => {
    expect(parseDurationMs("0m")).toBeUndefined();
    expect(parseDurationMs("-3d")).toBeUndefined();
  });

  it("rejects an unrecognised unit or shape", () => {
    expect(parseDurationMs("3w")).toBeUndefined();
    expect(parseDurationMs("soon")).toBeUndefined();
    expect(parseDurationMs("")).toBeUndefined();
  });
});

describe("formatDuration", () => {
  it("is parseDurationMs's inverse", () => {
    expect(formatDuration(2, "h")).toBe("2h");
    expect(parseDurationMs(formatDuration(2, "h"))).toBe(parseDurationMs("2h"));
  });
});

describe("durationUnitsFor", () => {
  it("offers all three units for a timestamp field", () => {
    expect(durationUnitsFor("timestamp")).toEqual(["m", "h", "d"]);
  });

  it("offers days only for a date field", () => {
    expect(durationUnitsFor("date")).toEqual(["d"]);
  });
});

describe("isBelowAlarmInterval", () => {
  const alarmIntervalMs = 15 * 60_000;

  it("warns when the duration is shorter than the alarm interval", () => {
    expect(isBelowAlarmInterval("5m", alarmIntervalMs)).toBe(true);
  });

  it("does not warn at or above the alarm interval", () => {
    expect(isBelowAlarmInterval("15m", alarmIntervalMs)).toBe(false);
    expect(isBelowAlarmInterval("1h", alarmIntervalMs)).toBe(false);
  });

  it("warns nothing for an unparseable duration — that is #133's save-time rejection", () => {
    expect(isBelowAlarmInterval("soon", alarmIntervalMs)).toBe(false);
  });
});
