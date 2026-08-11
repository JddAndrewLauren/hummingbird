import { describe, expect, it } from "vitest";
import { hasPriority, priorityLabel, priorityRank, PRIORITY_OPTIONS } from "./priority";

// The wire encoding (`items.priority`, ADR-0009) is Linear's own: 0 means
// "no priority" and 1..4 are Urgent..Low — inverted (1 is most urgent, not
// least) and holed (0 is not "between Low and Urgent", it sorts after
// everything). `priorityRank` is what a display sort must go through
// instead of the raw number — see `frontier-order.test.ts` for the sort
// itself.

describe("priorityRank", () => {
  it("ranks Urgent (1) first and No priority (0) last, in between the raw value is inverted", () => {
    const ranked = [0, 4, 1, 3, 2]
      .slice()
      .sort((a, b) => priorityRank(a) - priorityRank(b));
    expect(ranked).toEqual([1, 2, 3, 4, 0]);
  });

  it("a naive ascending sort on the raw wire value would be wrong twice over", () => {
    const naive = [0, 4, 1, 3, 2].slice().sort((a, b) => a - b);
    expect(naive).not.toEqual([1, 2, 3, 4, 0]);
  });

  it("degrades an out-of-range value to the same rank as 'no priority' rather than throwing", () => {
    expect(priorityRank(9)).toBe(priorityRank(0));
    expect(priorityRank(-1)).toBe(priorityRank(0));
  });
});

describe("priorityLabel", () => {
  it.each<[number, string]>([
    [0, "No priority"],
    [1, "Urgent"],
    [2, "High"],
    [3, "Medium"],
    [4, "Low"],
  ])("labels %i as %s", (raw, label) => {
    expect(priorityLabel(raw)).toBe(label);
  });

  it("labels an unrecognised value as 'No priority', never the raw number", () => {
    expect(priorityLabel(9)).toBe("No priority");
  });
});

describe("PRIORITY_OPTIONS", () => {
  it("offers all five values, most urgent first and No priority last", () => {
    // Not the raw ascending 0..4 the column stores: that encoding is inverted
    // and holed, so a form rendering it in numeric order would put
    // "No priority" above "Urgent".
    expect(PRIORITY_OPTIONS).toEqual([
      { value: "1", label: "Urgent" },
      { value: "2", label: "High" },
      { value: "3", label: "Medium" },
      { value: "4", label: "Low" },
      { value: "0", label: "No priority" },
    ]);
  });
});

describe("hasPriority", () => {
  it("is false for an explicit 0 (No priority)", () => {
    expect(hasPriority(0)).toBe(false);
  });

  it.each([1, 2, 3, 4])("is true for a real priority (%i)", (raw) => {
    expect(hasPriority(raw)).toBe(true);
  });

  it("is false for an out-of-range value, matching its 'No priority' label rather than a bare !== 0 check", () => {
    // PR #200 review: `raw !== 0` alone would say `true` here even though
    // `priorityLabel` renders it as "No priority" — a chip that says
    // "No priority" is noise.
    expect(hasPriority(9)).toBe(false);
  });
});
