import { describe, expect, it } from "vitest";
import {
  awaitingProjectCreate,
  buildTriageEdits,
  draftFromItem,
  effectiveDraft,
  hasTriageEdits,
  triageDraftProblems,
} from "./triage-form";
import { itemDTO, projectDTO } from "../test/component";
import type { TaskProjectResult } from "../store/store";

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

  // #631, ADR-0030 decision 3: copy-at-mint. The four combinations the
  // acceptance names — draft context present/absent × project default
  // present/absent — each against a promotion (`item.projectId` is `null`,
  // `draft.projectId` names the project being entered), so the transition
  // gate is satisfied in every case and the four differ only on the axes the
  // acceptance is about.
  describe("copy-at-mint: the project's default_context at promotion", () => {
    const withDefault = projectDTO({ id: "p1", defaultContext: "@computer" });
    const withoutDefault = projectDTO({ id: "p1", defaultContext: null });

    it("copies the project's default onto a context-less draft", () => {
      const draft = { ...draftFromItem(item), projectId: "p1" };
      expect(buildTriageEdits(draft, item, withDefault)).toEqual({
        projectId: "p1",
        context: "@computer",
      });
    });

    it("sends nothing for context when the project carries no default", () => {
      const draft = { ...draftFromItem(item), projectId: "p1" };
      expect(buildTriageEdits(draft, item, withoutDefault)).toEqual({
        projectId: "p1",
      });
    });

    it("a typed context wins over a default the project carries", () => {
      const draft = { ...draftFromItem(item), projectId: "p1", context: "@errands" };
      expect(buildTriageEdits(draft, item, withDefault)).toEqual({
        projectId: "p1",
        context: "@errands",
      });
    });

    it("a typed context is sent as-is with no project default to compete with", () => {
      const draft = { ...draftFromItem(item), projectId: "p1", context: "@errands" };
      expect(buildTriageEdits(draft, item, withoutDefault)).toEqual({
        projectId: "p1",
        context: "@errands",
      });
    });

    it("never copies onto an item already sitting in the project — nothing retroactive", () => {
      // The item is already in `p1` and already context-less; this call
      // changes an unrelated field (title) and touches neither context nor
      // project. `item.projectId === project.id`, so the transition gate
      // never opens.
      const already = itemDTO({ id: "i2", title: "old title", projectId: "p1", context: null });
      const draft = { ...draftFromItem(already), title: "new title" };
      expect(buildTriageEdits(draft, already, withDefault)).toEqual({
        title: "new title",
      });
    });

    it("copies at promotion onto an item that entered triage already carrying the project", () => {
      // The capture box has its own project picker and its own context
      // field, both optional, so an item can reach triage tagged `p1` with
      // no context — never having crossed the assignment gate. Promotion is
      // ADR-0030 decision 3's entry point 1, and it is the only copy point
      // this item will ever meet.
      const tagged = itemDTO({ id: "i4", projectId: "p1", context: null });
      const draft = draftFromItem(tagged);
      expect(buildTriageEdits(draft, tagged, withDefault, true)).toEqual({
        context: "@computer",
      });
    });

    it("copies nothing at promotion when the item already carries a context", () => {
      const tagged = itemDTO({ id: "i5", projectId: "p1", context: "@errands" });
      expect(buildTriageEdits(draftFromItem(tagged), tagged, withDefault, true)).toEqual({});
    });

    it("copies nothing on an ordinary save of that same item — only promotion counts", () => {
      const tagged = itemDTO({ id: "i4", projectId: "p1", context: null });
      expect(buildTriageEdits(draftFromItem(tagged), tagged, withDefault)).toEqual({});
    });

    it("keeps an explicit clear at promotion too", () => {
      const tagged = itemDTO({ id: "i6", projectId: "p1", context: "@errands" });
      const draft = { ...draftFromItem(tagged), context: "" };
      expect(buildTriageEdits(draft, tagged, withDefault, true)).toEqual({ context: null });
    });

    it("does nothing extra when no project is passed at all", () => {
      const draft = { ...draftFromItem(item), projectId: "p1" };
      expect(buildTriageEdits(draft, item, null)).toEqual({
        projectId: "p1",
      });
      expect(buildTriageEdits(draft, item)).toEqual({
        projectId: "p1",
      });
    });

    it("keeps an explicit clear when the human also moves the item into a project in the same save", () => {
      // The bug this guards: an item carrying `@errands` whose human deletes
      // the context AND picks a project in one save must end up with the
      // context cleared, not silently re-filled by the project's default —
      // the human's clear is human-typed and always wins, same as a typed
      // value above.
      const stocked = itemDTO({ id: "i3", context: "@errands", projectId: null });
      const draft = { ...draftFromItem(stocked), context: "", projectId: "p1" };
      expect(buildTriageEdits(draft, stocked, withDefault)).toEqual({
        projectId: "p1",
        context: null,
      });
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

describe("awaitingProjectCreate", () => {
  function write(kind: TaskProjectResult["kind"], overrides: Partial<TaskProjectResult> = {}): TaskProjectResult {
    return { seed: "s1", projectId: null, kind, error: null, ...overrides };
  }

  it("waits on a minted id the row has not seen, and stops once it lands", () => {
    const ok = write("ok", { projectId: "p-new" });
    expect(awaitingProjectCreate([], ok)).toBe(true);
    expect(awaitingProjectCreate([projectDTO({ id: "p-new" })], ok)).toBe(false);
  });

  it("waits on nothing before the first write, or after one that did not go through", () => {
    expect(awaitingProjectCreate([], null)).toBe(false);
    expect(awaitingProjectCreate([], write("failed", { error: "boom" }))).toBe(false);
    expect(awaitingProjectCreate([], write("busy"))).toBe(false);
  });
});
