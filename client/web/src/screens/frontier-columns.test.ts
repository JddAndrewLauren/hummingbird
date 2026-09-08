// #402's acceptance criteria for the grouping module, restated so the file
// says what it is pinning:
//
//   * "The unassigned bucket sorts last on every axis; fullest column first"
//   * "Within-column order is `orderFrontier`, unchanged" — which this module
//     honours by *preserving input order* inside every bucket, so the caller
//     orders once and there is no second ordering function here to drift
//   * "Grouping is a pure sibling module: input never mutated, same input to
//     same output, no ambient clock"
//
// The last one has since been narrowed rather than dropped. Four of the five
// axes are not time-varying and cannot read the clock at all; `urgency` is
// one entirely *because* it reads a deadline against a clock, and takes that
// clock as a parameter — so "no ambient clock" is still the property, and
// `groupFrontier`'s `nowMs` argument is what keeps it one.
//
// Rewritten from `frontier-groups.test.ts` (deleted with its module) rather
// than dropped: every case that file pinned for the one project axis is here,
// generalised to four.

import { describe, expect, it } from "vitest";
import type { ProjectDTO, TaskItemDTO } from "../store/protocol";
import {
  CALM_ORDERS,
  DEFAULT_CALM_ORDER,
  DEFAULT_FRONTIER_AXIS,
  FRONTIER_AXES,
  groupFrontier,
} from "./frontier-columns";

/** The wall clock every case below reads against — local, never UTC, since
 * that is the frame `groupFrontier` renders `nowMs` into. Bands: a deadline
 * before this is `overdue`, within 24h `now`, within 3 days `soon`, beyond
 * that (or absent) `calm`. */
const NOW_MS = Date.parse("2026-08-13T12:00:00");

function item(overrides: Partial<TaskItemDTO> = {}): TaskItemDTO {
  return {
    id: "id-0",
    seq: null,
    title: "untitled",
    description: null,
    stage: "ready",
    size: null,
    energy: null,
    context: null,
    priority: 0,
    projectId: null,
    projectPos: null,
    deadline: null,
    scheduledDate: null,
    source: null,
    sourceKey: null,
    sourceUrl: null,
    vaultPath: null,
    linkUrl: null,
    linkLabel: null,
    archivedAt: null,
    createdAt: 1,
    updatedAt: 1,
    version: 0,
    pending: false,
    ...overrides,
  };
}

function project(overrides: Partial<ProjectDTO> = {}): ProjectDTO {
  return {
    id: "p-0",
    name: "untitled project",
    githubRepo: null,
    defaultContext: null,
    archivedAt: null,
    createdAt: 1,
    updatedAt: 1,
    version: 0,
    ...overrides,
  };
}

/** The field each axis reads, so the four-axis cases below can be written
 * once rather than four times. */
const FIELD: Record<(typeof FIELD_AXES)[number], keyof TaskItemDTO> = {
  context: "context",
  project: "projectId",
  size: "size",
  energy: "energy",
};

/** The axes whose value is a field on the item, so a case can set it. The
 * `urgency` axis is excluded on purpose: its value is derived from the
 * deadline, it has no no-value column to place last (urgency is total) and
 * its columns are severity-ordered rather than fullest-first, so every
 * generalised case below would be asserting something untrue of it. Its own
 * describe block states each of those instead. */
const FIELD_AXES = ["context", "project", "size", "energy"] as const;

describe("groupFrontier — the axis vocabulary", () => {
  it("offers the four axes ADR-0021 licensed plus urgency, with context the default", () => {
    // The first four are CONTEXT.md's own "size, energy and context" plus
    // project, and deliberately exclude the delegation axis — a two-valued
    // marker whose absence is the default would yield one column plus "the
    // rest". `urgency` is decision 1's own amendment; it is last because the
    // switch's order is not a ranking and context still leads as the default.
    expect(FRONTIER_AXES).toEqual(["context", "project", "size", "energy", "urgency"]);
    expect(DEFAULT_FRONTIER_AXIS).toBe("context");
  });

  it("offers both calm-order directions, oldest the default", () => {
    expect(CALM_ORDERS).toEqual(["oldest", "newest"]);
    expect(DEFAULT_CALM_ORDER).toBe("oldest");
  });
});

describe("groupFrontier — bucketing, on every axis", () => {
  it("collects items by their value on the live axis, preserving input order", () => {
    for (const axis of FIELD_AXES) {
      const a = item({ id: "a", [FIELD[axis]]: "x" });
      const b = item({ id: "b", [FIELD[axis]]: "y" });
      const c = item({ id: "c", [FIELD[axis]]: "x" });

      const columns = groupFrontier([a, b, c], axis, [], NOW_MS);

      expect(columns.map((column) => column.value)).toEqual(["x", "y"]);
      // Input order inside the bucket — a and c in the order given, never
      // re-sorted, because `orderFrontier` already decided it.
      expect(columns[0].items).toEqual([a, c]);
      expect(columns[1].items).toEqual([b]);
    }
  });

  it("buckets items naming no value on the axis under a null column", () => {
    for (const axis of FIELD_AXES) {
      const named = item({ id: "a", [FIELD[axis]]: "x" });
      const unnamed = item({ id: "b" });

      const columns = groupFrontier([named, unnamed], axis, [], NOW_MS);

      expect(columns.map((column) => column.value)).toEqual(["x", null]);
      expect(columns[1].label).toBeNull();
    }
  });

  it("sorts the unnamed column last on every axis, even when it comes first in", () => {
    for (const axis of FIELD_AXES) {
      const unnamed = item({ id: "a" });
      const named = item({ id: "b", [FIELD[axis]]: "x" });

      const columns = groupFrontier([unnamed, named], axis, [], NOW_MS);

      expect(columns.map((column) => column.value)).toEqual(["x", null]);
    }
  });

  it("puts the fullest column first, whatever order the input arrived in", () => {
    for (const axis of FIELD_AXES) {
      const thin = item({ id: "a", [FIELD[axis]]: "thin" });
      const fat1 = item({ id: "b", [FIELD[axis]]: "fat" });
      const fat2 = item({ id: "c", [FIELD[axis]]: "fat" });

      // "thin" is seen first; "fat" still leads because it is fuller.
      const columns = groupFrontier([thin, fat1, fat2], axis, [], NOW_MS);

      expect(columns.map((column) => column.value)).toEqual(["fat", "thin"]);
    }
  });

  it("keeps a fuller column ahead of the unnamed one, and the unnamed one last even when it is fullest", () => {
    // The unnamed bucket's last place is unconditional — it is not merely
    // "sorted by count and happens to lose".
    const unnamed1 = item({ id: "a" });
    const unnamed2 = item({ id: "b" });
    const unnamed3 = item({ id: "c" });
    const named = item({ id: "d", context: "@computer" });

    const columns = groupFrontier([unnamed1, unnamed2, unnamed3, named], "context", [], NOW_MS);

    expect(columns.map((column) => column.value)).toEqual(["@computer", null]);
    expect(columns[1].items).toHaveLength(3);
  });

  it("breaks a tie between equal columns by first appearance, not arbitrarily", () => {
    const first = item({ id: "a", context: "@phone" });
    const second = item({ id: "b", context: "@computer" });

    const columns = groupFrontier([first, second], "context", [], NOW_MS);

    expect(columns.map((column) => column.value)).toEqual(["@phone", "@computer"]);
  });

  it("folds an empty-string axis value into the no-value column, keeping it single", () => {
    // `items.context` is `TEXT` with no CHECK and no server-side empty
    // rejection, so an API writer can land `""` even though every in-app writer
    // normalises it to `null`. Two no-value columns would share the caller's
    // `value ?? ""` key: duplicate React key, one collapse entry for two
    // columns, and a heading with no accessible name.
    for (const axis of FIELD_AXES) {
      const empty = item({ id: "a", [FIELD[axis]]: "" });
      const absent = item({ id: "b" });
      const named = item({ id: "c", [FIELD[axis]]: "x" });

      const columns = groupFrontier([empty, absent, named], axis, [], NOW_MS);

      expect(columns.map((column) => column.value)).toEqual(["x", null]);
      // Both landed in the one no-value column, and it still sorts last even
      // though it is now the fuller of the two.
      expect(columns[1].items.map((entry) => entry.id)).toEqual(["a", "b"]);
    }
  });

  it("gives every column a key no other column can produce", () => {
    // The property the fold above exists to protect, stated directly.
    const columns = groupFrontier(
      [
        item({ id: "a", context: "" }),
        item({ id: "b", context: null }),
        item({ id: "c", context: "@computer" }),
      ],
      "context",
      [],
      NOW_MS,
    );
    const keys = columns.map((column) => column.value ?? "");
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("returns no columns for an empty frontier", () => {
    for (const axis of FRONTIER_AXES) {
      expect(groupFrontier([], axis, [], NOW_MS)).toEqual([]);
    }
  });
});

describe("groupFrontier — the urgency axis", () => {
  // ADR-0021 decision 1's own amendment: the fifth axis. Every claim here is
  // one the four field-backed axes do not share.
  const dated = (id: string, deadline: string) => item({ id, deadline });

  it("emits its bands in severity order, never fullest first", () => {
    // `calm` is deliberately the fullest, so fullest-first would put it
    // first if this axis had not opted out of that rule.
    const columns = groupFrontier(
      [
        item({ id: "calm-1" }),
        item({ id: "calm-2" }),
        item({ id: "calm-3" }),
        dated("soon", "2026-08-15T12:00"),
        dated("now", "2026-08-13T18:00"),
        dated("overdue", "2026-08-12T12:00"),
      ],
      "urgency",
      [],
      NOW_MS,
    );

    expect(columns.map((column) => column.value)).toEqual(["overdue", "now", "soon", "calm"]);
  });

  it("omits a band nothing is in", () => {
    const columns = groupFrontier(
      [dated("overdue", "2026-08-12T12:00"), item({ id: "calm" })],
      "urgency",
      [],
      NOW_MS,
    );

    expect(columns.map((column) => column.value)).toEqual(["overdue", "calm"]);
  });

  it("never yields a no-value column — urgency is total", () => {
    // The one axis where "the no-value column always last" never fires: an
    // item with no deadline, and one whose deadline will not parse, both
    // read as `calm` rather than as no value at all.
    const columns = groupFrontier(
      [item({ id: "no-deadline" }), dated("unparseable", "sometime next week")],
      "urgency",
      [],
      NOW_MS,
    );

    expect(columns.map((column) => column.value)).toEqual(["calm"]);
    expect(columns[0].items).toHaveLength(2);
  });

  it("orders the calm column oldest first by default", () => {
    const columns = groupFrontier(
      [
        item({ id: "c", createdAt: 3_000 }),
        item({ id: "a", createdAt: 1_000 }),
        item({ id: "b", createdAt: 2_000 }),
      ],
      "urgency",
      [],
      NOW_MS,
    );

    expect(columns[0].items.map((entry) => entry.id)).toEqual(["a", "b", "c"]);
  });

  it("reverses the calm column when the reader asks for newest first", () => {
    const columns = groupFrontier(
      [
        item({ id: "c", createdAt: 3_000 }),
        item({ id: "a", createdAt: 1_000 }),
        item({ id: "b", createdAt: 2_000 }),
      ],
      "urgency",
      [],
      NOW_MS,
      "newest",
    );

    expect(columns[0].items.map((entry) => entry.id)).toEqual(["c", "b", "a"]);
  });

  it("leaves every dated band in the caller's order, whichever direction is asked", () => {
    // Only `calm` is re-sorted. The others keep `orderFrontier`'s order,
    // `createdAt` notwithstanding.
    const first = item({ id: "first", deadline: "2026-08-12T12:00", createdAt: 9_000 });
    const second = item({ id: "second", deadline: "2026-08-12T09:00", createdAt: 1_000 });

    for (const order of CALM_ORDERS) {
      const columns = groupFrontier([first, second], "urgency", [], NOW_MS, order);
      expect(columns[0].items.map((entry) => entry.id)).toEqual(["first", "second"]);
    }
  });

  it("bands against the clock it is given, not an ambient one", () => {
    const a = dated("a", "2026-08-13T18:00");

    expect(groupFrontier([a], "urgency", [], NOW_MS)[0].value).toBe("now");
    // The same item, read a week later.
    expect(
      groupFrontier([a], "urgency", [], Date.parse("2026-08-20T12:00:00"))[0].value,
    ).toBe("overdue");
  });
});

describe("groupFrontier — the project axis's wire hop", () => {
  // PR #200's review found an earlier `frontier-groups.ts` rendering a raw
  // project uuid as a heading. The hop that fixed it is kept here.
  it("resolves a column's real name from the given project list", () => {
    const a = item({ id: "a", projectId: "p-1" });

    const columns = groupFrontier([a], "project", [
      project({ id: "p-1", name: "Ship the release" }),
    ], NOW_MS);

    expect(columns).toEqual([{ value: "p-1", label: "Ship the release", items: [a] }]);
  });

  it("falls back to a null label for a projectId not (yet) in the project list", () => {
    const a = item({ id: "a", projectId: "unknown-id" });

    const columns = groupFrontier([a], "project", [
      project({ id: "p-1", name: "Ship the release" }),
    ], NOW_MS);

    expect(columns).toEqual([{ value: "unknown-id", label: null, items: [a] }]);
  });

  it("labels every other axis with the value itself — no hop to make", () => {
    for (const axis of ["context", "size", "energy"] as const) {
      const a = item({ id: "a", [FIELD[axis]]: "raw-value" });

      const columns = groupFrontier([a], axis, [project({ id: "raw-value", name: "Not this" })], NOW_MS);

      // Notably NOT the project name, even though a project shares the id:
      // the hop belongs to the project axis alone.
      expect(columns[0].label).toBe("raw-value");
    }
  });
});

describe("groupFrontier — purity", () => {
  it("never mutates its input array or its items", () => {
    const input = [item({ id: "a", context: "@computer" }), item({ id: "b" })];
    const arrayCopy = [...input];
    const itemsCopy = structuredClone(input);

    groupFrontier(input, "context", [project()], NOW_MS);

    expect(input).toEqual(arrayCopy);
    expect(input).toEqual(itemsCopy);
  });

  it("returns fresh column objects rather than aliasing an internal cache", () => {
    const input = [item({ id: "a", context: "@computer" })];

    const first = groupFrontier(input, "context", [], NOW_MS);
    const second = groupFrontier(input, "context", [], NOW_MS);

    expect(first).toEqual(second);
    expect(first).not.toBe(second);
    expect(first[0].items).not.toBe(second[0].items);
  });

  it("gives the same output for the same input, on every axis", () => {
    const input = [
      item({ id: "a", context: "@computer", size: "quick", energy: "low", projectId: "p-1" }),
      item({ id: "b", context: "@computer", size: "deep", energy: "high", projectId: null }),
      item({ id: "c" }),
    ];
    const projects = [project({ id: "p-1", name: "Ship the release" })];

    for (const axis of FRONTIER_AXES) {
      expect(groupFrontier(input, axis, projects, NOW_MS)).toEqual(
        groupFrontier(input, axis, projects, NOW_MS),
      );
    }
  });

  it("accounts for every input item exactly once, on every axis", () => {
    const input = [
      item({ id: "a", context: "@computer" }),
      item({ id: "b", context: "@phone" }),
      item({ id: "c" }),
      item({ id: "d", context: "@computer" }),
    ];

    for (const axis of FRONTIER_AXES) {
      const ids = groupFrontier(input, axis, [], NOW_MS)
        .flatMap((column) => column.items)
        .map((entry) => entry.id)
        .sort();
      expect(ids).toEqual(["a", "b", "c", "d"]);
    }
  });
});
