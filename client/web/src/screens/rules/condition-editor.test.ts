import { describe, expect, it } from "vitest";
import { newCondition, retypeCondition, toggleNegate, widgetFor } from "./condition-editor";

describe("widgetFor", () => {
  it("gives chips for any string_list field regardless of operator", () => {
    expect(widgetFor("to", "string_list", "eq")).toBe("chips");
    expect(widgetFor("labels", "string_list", "contains")).toBe("chips");
  });

  it("gives a duration picker for within_next/within_last on an ordinary field", () => {
    expect(widgetFor("received_at", "timestamp", "within_last")).toBe("duration");
    expect(widgetFor("scheduled_date", "date", "within_next")).toBe("duration");
  });

  it("gives a date/time picker for within_next/within_last on deadline specifically", () => {
    expect(widgetFor("deadline", "timestamp", "within_next")).toBe("datetime");
    expect(widgetFor("deadline", "timestamp", "within_last")).toBe("datetime");
  });

  it("gives a boolean control for a bool field", () => {
    expect(widgetFor("has_attachment", "bool", "is")).toBe("boolean");
  });

  it("gives a number control for a number field", () => {
    expect(widgetFor("priority", "number", "eq")).toBe("number");
  });

  it("falls back to text for a plain string field", () => {
    expect(widgetFor("subject", "string", "contains")).toBe("text");
  });
});

describe("newCondition", () => {
  it("starts at the field type's default operator", () => {
    expect(newCondition("subject", "string").op).toBe("eq");
    expect(newCondition("priority", "number").op).toBe("eq");
    expect(newCondition("has_attachment", "bool").op).toBe("is");
  });

  it("starts negate off and value empty in the type's own shape", () => {
    expect(newCondition("subject", "string")).toEqual({
      field: "subject",
      op: "eq",
      value: "",
      negate: false,
    });
    expect(newCondition("to", "string_list").value).toEqual([]);
  });
});

describe("retypeCondition", () => {
  it("leaves an already-legal operator and value alone", () => {
    const condition = { field: "subject", op: "contains", value: "urgent", negate: true };
    expect(retypeCondition(condition, "string")).toBe(condition);
  });

  it("resets to the new type's default operator and empty value when no longer legal", () => {
    const condition = { field: "subject", op: "contains", value: "urgent", negate: false };
    expect(retypeCondition(condition, "number")).toEqual({
      field: "subject",
      op: "eq",
      value: "",
      negate: false,
    });
  });
});

describe("toggleNegate", () => {
  it("flips the flag without touching anything else", () => {
    const condition = { field: "subject", op: "contains", value: "urgent", negate: false };
    expect(toggleNegate(condition)).toEqual({ ...condition, negate: true });
    expect(toggleNegate(toggleNegate(condition))).toEqual(condition);
  });
});
