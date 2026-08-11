import { describe, expect, it } from "vitest";
import {
  buildTriageEdits,
  draftFromItem,
  effectiveDraft,
  hasTriageEdits,
  triageDraftProblems,
} from "./triage-form";
import { itemDTO } from "../test/component";

// The draft is seeded from the item now, so every assertion here is about a
// DIFF: what changed against the row on screen. That is what makes "an
// untouched field is not in the mutation", "an emptied field is a clear" and
// "everything but the source is editable" all one rule instead of three.

const item = itemDTO({
  id: "i1",
  title: "vague thing",
  description: null,
  size: null,
  energy: null,
  context: null,
  priority: 0,
  projectId: null,
  deadline: null,
  scheduledDate: null,
});

describe("draftFromItem", () => {
  it("shows what the item holds, with every null as an empty control", () => {
    const stocked = itemDTO({
      title: "Order the worktop",
      description: "oak, 3m",
      size: "deep",
      energy: "high",
      context: "@computer",
      priority: 2,
      projectId: "p1",
      deadline: "2026-08-14T09:30",
      scheduledDate: "2026-08-12",
    });
    expect(draftFromItem(stocked)).toEqual({
      title: "Order the worktop",
      description: "oak, 3m",
      size: "deep",
      energy: "high",
      context: "@computer",
      priority: "2",
      projectId: "p1",
      deadline: "2026-08-14T09:30",
      scheduledDate: "2026-08-12",
    });
    expect(draftFromItem(item)).toEqual({
      title: "vague thing",
      description: "",
      size: "",
      energy: "",
      context: "",
      priority: "0",
      projectId: "",
      deadline: "",
      scheduledDate: "",
    });
  });
});

describe("effectiveDraft", () => {
  it("lays typing over the item, and lets an untouched field follow the item", () => {
    const touched = { context: "@errands" };
    expect(effectiveDraft(item, touched).context).toBe("@errands");

    // The hazard this shape exists to remove: another device's edit arrives on
    // a field nobody is typing in, and it shows through rather than sitting
    // under a stale captured draft that would push the old value back.
    const pulled = { ...item, size: "deep" as const, context: "@home" as const };
    const draft = effectiveDraft(pulled, touched);
    expect(draft.size).toBe("deep");
    expect(draft.context).toBe("@errands");
  });
});

describe("buildTriageEdits", () => {
  it("sends nothing at all for a draft nobody changed", () => {
    // A bare promotion. Not `{title: null, …}` — that would now mean "clear
    // every one of these fields".
    expect(buildTriageEdits(draftFromItem(item), item)).toEqual({});
    expect(hasTriageEdits(draftFromItem(item), item)).toBe(false);
  });

  it("carries every changed field in one object, and only the changed ones", () => {
    const draft = {
      ...draftFromItem(item),
      title: "Order the worktop",
      description: "oak, 3m",
      projectId: "p1",
      size: "deep" as const,
      energy: "high" as const,
      context: "@computer",
      priority: "2",
      deadline: "2026-08-14",
      scheduledDate: "2026-08-12",
    };
    expect(buildTriageEdits(draft, item)).toEqual({
      title: "Order the worktop",
      description: "oak, 3m",
      projectId: "p1",
      size: "deep",
      energy: "high",
      context: "@computer",
      priority: 2,
      deadline: "2026-08-14",
      scheduledDate: "2026-08-12",
    });
    expect(hasTriageEdits(draft, item)).toBe(true);
  });

  it("sends an emptied field as an explicit null — a clear, not an omission", () => {
    const stocked = itemDTO({
      description: "oak, 3m",
      size: "deep",
      energy: "high",
      context: "@computer",
      projectId: "p1",
      deadline: "2026-08-14",
      scheduledDate: "2026-08-12",
    });
    const emptied = {
      ...draftFromItem(stocked),
      description: "",
      size: "" as const,
      energy: "" as const,
      context: "",
      projectId: "",
      deadline: "",
      scheduledDate: "",
    };
    expect(buildTriageEdits(emptied, stocked)).toEqual({
      description: null,
      size: null,
      energy: null,
      context: null,
      projectId: null,
      deadline: null,
      scheduledDate: null,
    });
  });

  it("never sends a blanked title, which is NOT NULL and has no cleared state", () => {
    const draft = { ...draftFromItem(item), title: "   " };
    expect(buildTriageEdits(draft, item)).toEqual({});
  });

  it("trims a title and a description before comparing and sending", () => {
    expect(buildTriageEdits({ ...draftFromItem(item), title: "  buy milk  " }, item)).toEqual({
      title: "buy milk",
    });
    // Trimmed to the value the item already holds, so it is not an edit at all.
    const stocked = itemDTO({ description: "oak, 3m" });
    expect(
      buildTriageEdits({ ...draftFromItem(stocked), description: "  oak, 3m  " }, stocked),
    ).toEqual({});
  });

  it("sends priority as the number the column stores, and 0 as a real value", () => {
    const stocked = itemDTO({ priority: 3 });
    expect(buildTriageEdits({ ...draftFromItem(stocked), priority: "0" }, stocked)).toEqual({
      priority: 0,
    });
    expect(buildTriageEdits({ ...draftFromItem(item), priority: "1" }, item)).toEqual({
      priority: 1,
    });
  });
});

describe("triageDraftProblems", () => {
  it("passes a draft with nothing wrong", () => {
    expect(triageDraftProblems(draftFromItem(item))).toEqual({});
  });

  it("refuses an empty title", () => {
    expect(triageDraftProblems({ ...draftFromItem(item), title: "  " })).toEqual({
      title: expect.any(String),
    });
  });

  it("accepts both deadline shapes and refuses everything else", () => {
    const base = draftFromItem(item);
    for (const deadline of ["", "2026-08-14", "2026-08-14T09:30", "2028-02-29"]) {
      expect(triageDraftProblems({ ...base, deadline })).toEqual({});
    }
    for (const deadline of [
      "2026-08-14T09:30:00",
      "2026-08-14T09:30Z",
      "14/08/2026",
      "2026-02-30",
      "2027-02-29",
      "2026-13-01",
      "2026-08-14T24:00",
      "09:30",
    ]) {
      expect(
        triageDraftProblems({ ...base, deadline }),
        `${deadline} must be refused`,
      ).toEqual({ deadline: expect.any(String) });
    }
  });

  it("refuses a scheduled date carrying a time — a do-date has no minute", () => {
    const base = draftFromItem(item);
    expect(triageDraftProblems({ ...base, scheduledDate: "2026-08-12" })).toEqual({});
    expect(triageDraftProblems({ ...base, scheduledDate: "2026-08-12T09:30" })).toEqual({
      scheduledDate: expect.any(String),
    });
  });

  it("refuses a priority outside the stored encoding", () => {
    const base = draftFromItem(item);
    for (const priority of ["0", "1", "2", "3", "4"]) {
      expect(triageDraftProblems({ ...base, priority })).toEqual({});
    }
    for (const priority of ["5", "-1", "", "high"]) {
      expect(triageDraftProblems({ ...base, priority })).toEqual({
        priority: expect.any(String),
      });
    }
  });
});
