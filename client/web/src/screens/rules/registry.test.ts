import { describe, expect, it } from "vitest";
import type { KindRegistryDTO } from "../../store/protocol";
import { fieldsForKind, fieldType, kindLabel, kindOptions } from "./registry";

const registry: KindRegistryDTO = {
  coreFields: [
    { name: "source", fieldType: "string" },
    { name: "occurred_at", fieldType: "timestamp" },
  ],
  kinds: [
    {
      key: "email",
      mints: true,
      fields: [
        { name: "subject", fieldType: "string" },
        { name: "to", fieldType: "string_list" },
        // A core-name collision: must not be listed twice.
        { name: "source", fieldType: "string" },
      ],
    },
    { key: "alert_raised", mints: false, fields: [] },
  ],
  alarmIntervalMs: 900_000,
};

describe("kindOptions", () => {
  it("puts any-kind first, then every registry entry in declared order", () => {
    expect(kindOptions(registry)).toEqual([
      { key: null, label: "Any kind" },
      { key: "email", label: "Email" },
      { key: "alert_raised", label: "Alert raised" },
    ]);
  });
});

describe("kindLabel", () => {
  it("falls back to the raw key for an unrecognised one", () => {
    expect(kindLabel("weather_alert")).toBe("weather_alert");
  });
});

describe("fieldsForKind", () => {
  it("narrows to the Event core for any kind", () => {
    expect(fieldsForKind(registry, null)).toEqual(registry.coreFields);
  });

  it("offers the core plus a named kind's own fields, core first", () => {
    const fields = fieldsForKind(registry, "email");
    expect(fields.map((f) => f.name)).toEqual(["source", "occurred_at", "subject", "to"]);
  });

  it("never lists a kind field that collides with a core name twice", () => {
    const fields = fieldsForKind(registry, "email");
    expect(fields.filter((f) => f.name === "source")).toHaveLength(1);
  });

  it("falls back to the core alone for a kind this build's registry has never heard of", () => {
    expect(fieldsForKind(registry, "weather_alert")).toEqual(registry.coreFields);
  });
});

describe("fieldType", () => {
  it("resolves a field's declared type within the offered list", () => {
    expect(fieldType(registry, "email", "subject")).toBe("string");
    expect(fieldType(registry, "email", "source")).toBe("string");
  });

  it("is undefined for a field the offered list does not carry", () => {
    expect(fieldType(registry, null, "subject")).toBeUndefined();
  });
});
