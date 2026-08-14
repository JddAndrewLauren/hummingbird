// @vitest-environment jsdom

// The regression suite for PR #207's round-2 defect, and the reason this
// repo now has component tests at all.
//
// The defect: `applyItemAction` freezes `pending: true` onto the optimistic
// item. `"block"` moves an item to `Stage::Blocked`, which is outside BOTH
// `getFrontier` and `getBlocked` (S10's scope), so nothing ever replaced
// that frozen snapshot — the detail panel's Start/Cancel row rendered
// `disabled` forever and the item became functionally unreachable. It
// survived TWO review rounds because every piece of it was individually
// correct: `resolveFallbackPending` is unit-tested and right, `ItemRow` is
// right, `ItemDetailPanel` is right. Only the thread between them was
// broken, and a thread is exactly what typecheck cannot see.
//
// So these tests drive the whole lifecycle through the mounted screen: act,
// watch the item leave every live query, then walk `TaskState.pending` the
// way `worker-client.ts` really walks it, and assert the row comes back.

import { describe, expect, it, vi } from "vitest";
import { NowScreen } from "./NowScreen";
import { QUESTION_ORDER } from "./questions/contract";
import { QUESTIONS } from "./questions/registry";
import { CALENDAR_REQUEST_KEY, weekendWindow } from "./weekend-pane/weekend";
import {
  blockedEntryDTO,
  bindingDTO,
  fireEvent,
  itemDTO,
  paneReadDTO,
  paneSnapshotDTO,
  projectDTO,
  render,
  screen,
  taskState,
  wasteBody,
} from "../test/component";
import { DEMO_DATA } from "../fixtures/demo-data";
import { BINDING_KEY, SOURCE } from "./waste-pane/waste";
import type { CalendarReadDTO } from "../store/protocol";
import type { TaskState } from "../store/store";

const NOW_MS = 1_700_000_000_000;

/** A fresh in-memory `storage` per render, and the reason it is not optional.
 *
 * `NowScreen` falls back to the ambient `localStorage` when given no `storage`
 * prop, and since #403 the frontier persists its grouping axis there. Left to
 * the fallback, a test that switches the axis writes into storage every LATER
 * test in this file then reads — so the suite's outcome depends on test order
 * and on whether the runtime has a working `localStorage` at all. It does not
 * under this repo's local vitest (node reports "localStorage is not available
 * because --localstorage-file was not provided") and it does in CI, which is
 * how the same commit passed here and failed there. Hermetic per test instead;
 * `renderNow`'s own `rerender` deliberately keeps the SAME instance, because a
 * rerender is not a remount and must not reset what the screen restored. */
function memoryStorage(seed: Record<string, string> = {}) {
  const entries = { ...seed };
  return {
    entries,
    getItem: (key: string) => entries[key] ?? null,
    setItem: (key: string, value: string) => {
      entries[key] = value;
    },
    removeItem: (key: string) => {
      delete entries[key];
    },
  };
}

/** The frontier's column headings — every `h2` on the screen except the
 * standing-questions aside's own, which is a section header in the other
 * landmark entirely and is not a column. */
function columnHeadings(): HTMLElement[] {
  return screen
    .getAllByRole("heading", { level: 2 })
    .filter((heading) => heading.closest("aside") === null);
}

function renderNow(task: TaskState, selectedItemId: string | null = null) {
  const onAct = vi.fn();
  const onOpenItem = vi.fn();
  const onCloseItemDetail = vi.fn();
  const storage = memoryStorage();
  const view = render(
    <NowScreen
      demo={null}
      onScreen={() => {}}
      task={task}
      nowMs={NOW_MS}
      selectedItemId={selectedItemId}
      onOpenItem={onOpenItem}
      onCloseItemDetail={onCloseItemDetail}
      onAct={onAct}
      calendarReads={{}}
      calendarConnected={false}
      storage={storage}
    />,
  );
  const rerender = (next: TaskState, nextSelected: string | null = selectedItemId) =>
    view.rerender(
      <NowScreen
        demo={null}
        onScreen={() => {}}
        task={next}
        nowMs={NOW_MS}
        selectedItemId={nextSelected}
        onOpenItem={onOpenItem}
        onCloseItemDetail={onCloseItemDetail}
        onAct={onAct}
        calendarReads={{}}
        calendarConnected={false}
        storage={storage}
      />,
    );
  return { onAct, onOpenItem, onCloseItemDetail, rerender, storage };
}

// The guard for the defect above: a test that reached the ambient
// `localStorage` made this file's result depend on test order, and it only
// showed up in CI because the local runtime has no `localStorage` to leak
// through. Asserted structurally rather than trusted to a convention.
describe("NowScreen — test isolation", () => {
  it("never touches the ambient localStorage when given a storage prop", () => {
    const calls: string[] = [];
    const spy = {
      getItem: (key: string) => {
        calls.push(`get ${key}`);
        return null;
      },
      setItem: (key: string) => {
        calls.push(`set ${key}`);
      },
      removeItem: (key: string) => {
        calls.push(`remove ${key}`);
      },
    };
    const original = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
    Object.defineProperty(globalThis, "localStorage", { value: spy, configurable: true });
    try {
      renderNow(
        taskState({ frontier: [itemDTO({ id: "i1", title: "A thing", context: "@computer" })] }),
      );
      // Switching the axis is the write that used to leak across tests.
      fireEvent.click(screen.getByRole("button", { name: "Energy" }));
      expect(calls).toEqual([]);
    } finally {
      if (original) {
        Object.defineProperty(globalThis, "localStorage", original);
      } else {
        Reflect.deleteProperty(globalThis, "localStorage");
      }
    }
  });

  it("gives each render its own storage, so an axis switch cannot outlive its test", () => {
    const first = renderNow(taskState({ frontier: [itemDTO({ id: "i1", context: "@computer" })] }));
    fireEvent.click(screen.getByRole("button", { name: "Energy" }));
    expect(first.storage.entries["hb.now.frontier-axis"]).toBe("energy");

    const second = renderNow(taskState({ frontier: [itemDTO({ id: "i2", context: "@computer" })] }));
    expect("hb.now.frontier-axis" in second.storage.entries).toBe(false);
    // ...and the second render is back on the default axis.
    expect(screen.getByRole("button", { name: "Context", pressed: true })).toBeDefined();
  });
});

describe("NowScreen — the act lifecycle (PR #207 round 2)", () => {
  it("re-enables the action row once the queued act drains, for an item that left every live query", () => {
    const item = itemDTO({ id: "i1", title: "Renew the passport", stage: "ready" });
    const { onAct, rerender } = renderNow(taskState({ frontier: [item] }), "i1");

    // Ready offers start / block / cancel.
    const blockButton = screen.getByRole("button", { name: /mark blocked/i });
    fireEvent.click(blockButton);
    expect(onAct).toHaveBeenCalledWith("i1", "block");

    // The mutation is queued: the item is now Stage::Blocked, which neither
    // getFrontier nor getBlocked returns. The panel must stay open on the
    // optimistic projection rather than going blank.
    rerender(taskState({ frontier: [], blocked: [], pending: {} }));
    expect(screen.getByRole("heading", { name: "Renew the passport" })).toBeDefined();
    const startWhileQueued = screen.getByRole("button", { name: /^start$/i });
    expect(startWhileQueued.hasAttribute("disabled")).toBe(true);

    // `worker-client.ts` re-reads isPending on the ok actResult: still true.
    rerender(taskState({ frontier: [], blocked: [], pending: { i1: true } }));
    expect(screen.getByRole("button", { name: /^start$/i }).hasAttribute("disabled")).toBe(true);

    // The cycle drains it. THIS is the assertion the defect failed: the
    // frozen optimistic `pending: true` must not outlive the live read.
    rerender(taskState({ frontier: [], blocked: [], pending: { i1: false } }));
    expect(screen.getByRole("button", { name: /^start$/i }).hasAttribute("disabled")).toBe(false);
  });

  it("does not re-enable the row between two acts, while the second is still unconfirmed", () => {
    const item = itemDTO({ id: "i1", stage: "ready" });
    const { rerender } = renderNow(taskState({ frontier: [item] }), "i1");

    fireEvent.click(screen.getByRole("button", { name: /mark blocked/i }));
    rerender(taskState({ frontier: [], pending: { i1: true } }));
    rerender(taskState({ frontier: [], pending: { i1: false } }));

    // Second act, fired while the store still holds the FIRST act's drained
    // `false` — the row must disable immediately, not flicker enabled.
    // Starting a blocked item projects it to `in_progress`, so the row it
    // re-renders as is that stage's own (complete / block / cancel).
    fireEvent.click(screen.getByRole("button", { name: /^start$/i }));
    rerender(taskState({ frontier: [], pending: { i1: false } }));
    expect(screen.getByRole("button", { name: /complete/i }).hasAttribute("disabled")).toBe(true);

    // ...and it stays disabled until this act's own `true` has been seen.
    rerender(taskState({ frontier: [], pending: { i1: true } }));
    expect(screen.getByRole("button", { name: /complete/i }).hasAttribute("disabled")).toBe(true);
    rerender(taskState({ frontier: [], pending: { i1: false } }));
    expect(screen.getByRole("button", { name: /complete/i }).hasAttribute("disabled")).toBe(false);
  });

  it("drops a previous selection's optimistic item when a different item opens", () => {
    const one = itemDTO({ id: "i1", title: "First", stage: "ready" });
    const two = itemDTO({ id: "i2", title: "Second", stage: "ready" });
    const { rerender } = renderNow(taskState({ frontier: [one, two] }), "i1");

    fireEvent.click(screen.getByRole("button", { name: /mark blocked/i }));
    // i1 has left the frontier; i2 is opened instead.
    rerender(taskState({ frontier: [two], pending: { i1: true } }), "i2");

    expect(screen.getByRole("heading", { name: "Second" })).toBeDefined();
    expect(screen.queryByRole("heading", { name: "First" })).toBeNull();
    // i2 is live and unqueued, so its row is usable — the stale optimistic
    // `pending: true` from i1 must not have leaked across.
    expect(screen.getByRole("button", { name: /^start$/i }).hasAttribute("disabled")).toBe(false);
  });

  it("surfaces a failed act, and only for the item it belongs to", () => {
    const item = itemDTO({ id: "i1", stage: "ready" });
    const other = itemDTO({ id: "i2", stage: "ready" });
    const failure = {
      seed: "s",
      itemId: "i2",
      action: "start" as const,
      kind: "failed" as const,
      error: "Nope.",
    };
    const { rerender } = renderNow(
      taskState({ frontier: [item, other], lastAct: failure }),
      "i1",
    );
    // The failure belongs to i2; i1's panel must not wear it.
    expect(screen.queryByText("Nope.")).toBeNull();

    rerender(taskState({ frontier: [item, other], lastAct: failure }), "i2");
    expect(screen.getByText("Nope.")).toBeDefined();
  });

  it("announces a failed act to a screen reader", () => {
    // The danger-text colour was the whole signal, and the paragraph appears
    // with no other change on the page — so without a live region a failed
    // act says nothing at all to a non-sighted reader.
    const item = itemDTO({ id: "i1", stage: "ready" });
    renderNow(
      taskState({
        frontier: [item],
        lastAct: { seed: "s", itemId: "i1", action: "start", kind: "failed", error: "Nope." },
      }),
      "i1",
    );
    expect(screen.getByRole("alert").textContent).toBe("Nope.");
  });
});

describe("NowScreen — the frontier list", () => {
  it("marks a pending item as such, and leaves a confirmed one unmarked", () => {
    // Issue #108's acceptance criterion, which shipped dead once already
    // (PR #200: `requestIsPending` had no caller).
    renderNow(
      taskState({
        frontier: [
          itemDTO({ id: "i1", title: "Queued offline", pending: true }),
          itemDTO({ id: "i2", title: "Confirmed", pending: false }),
        ],
      }),
    );
    expect(screen.getAllByText("Pending")).toHaveLength(1);
  });

  // Rewritten from "groups by project name, with the unassigned group last"
  // (#402): project is now one of four axes rather than the only grouping, so
  // the same fact is asserted through the axis switch.
  it("groups by the live axis, with the unnamed column last", () => {
    renderNow(
      taskState({
        frontier: [
          itemDTO({ id: "i1", title: "Loose", projectId: null }),
          itemDTO({ id: "i2", title: "Owned", projectId: "p1" }),
        ],
        projects: [projectDTO({ id: "p1", name: "Kitchen rebuild" })],
      }),
    );

    // Context is the default axis, and neither item names one.
    expect(screen.getByRole("heading", { name: "No context" })).toBeDefined();
    expect(screen.queryByRole("heading", { name: "Kitchen rebuild" })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Project", pressed: false }));

    const headings = screen.getAllByRole("heading").map((node) => node.textContent);
    expect(headings.indexOf("Kitchen rebuild")).toBeGreaterThanOrEqual(0);
    expect(headings.indexOf("Kitchen rebuild")).toBeLessThan(headings.indexOf("No project"));
  });

  it("switches the axis across all four, and each one groups by its own field", () => {
    // One item per axis-value so every axis produces a *named* column — the
    // regression this guards is an axis silently reading the wrong field.
    renderNow(
      taskState({
        frontier: [
          itemDTO({
            id: "i1",
            title: "Rewire the lamp",
            context: "@garden",
            size: "deep",
            energy: "high",
            projectId: "p1",
          }),
        ],
        projects: [projectDTO({ id: "p1", name: "Kitchen rebuild" })],
      }),
    );

    for (const [axis, heading] of [
      ["Context", "@garden"],
      ["Project", "Kitchen rebuild"],
      ["Size", "deep"],
      ["Energy", "high"],
    ] as const) {
      fireEvent.click(screen.getByRole("button", { name: axis }));
      expect(screen.getByRole("heading", { name: heading })).toBeDefined();
    }
  });

  it("caps a column at six cards and says how many are hidden", () => {
    renderNow(
      taskState({
        frontier: Array.from({ length: 9 }, (_, index) =>
          itemDTO({ id: `i${index}`, title: `Action ${index}`, context: "@computer" }),
        ),
      }),
    );

    // Six of nine on screen, and the count never lies about the rest. Both
    // halves are asserted: `queryByText(...).toBeDefined()` would pass whether
    // the card were there or not, since `queryByText` returns `null` on a miss
    // and `toBeDefined()` accepts `null`.
    expect(screen.getByRole("heading", { name: "@computer" })).toBeDefined();
    expect(screen.getByText("Action 0")).toBeDefined();
    expect(screen.getByText("Action 5")).toBeDefined();
    expect(screen.queryByText("Action 6")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Show 3 more in @computer" }));
    expect(screen.getByText("Action 8")).toBeDefined();

    fireEvent.click(screen.getByRole("button", { name: "Show fewer in @computer" }));
    expect(screen.queryByText("Action 6")).toBeNull();
  });

  it("names the reveal toggle by its column, so two columns are not two identical buttons", () => {
    // #403's facet chips needed the same fix. Two columns hiding the same count
    // give two buttons whose visible text is identical, and nothing else ties
    // either to the column it belongs to.
    renderNow(
      taskState({
        frontier: [
          ...Array.from({ length: 8 }, (_, i) =>
            itemDTO({ id: `a${i}`, title: `A${i}`, context: "@computer" }),
          ),
          ...Array.from({ length: 8 }, (_, i) =>
            itemDTO({ id: `b${i}`, title: `B${i}`, context: "@phone" }),
          ),
        ],
      }),
    );

    expect(screen.getAllByText("2 more")).toHaveLength(2);
    expect(screen.getByRole("button", { name: "Show 2 more in @computer" })).toBeDefined();
    expect(screen.getByRole("button", { name: "Show 2 more in @phone" })).toBeDefined();
  });

  it("drops the reveal toggle when an expanded column falls back to the cap", () => {
    // Expand a 7-card column, then filter it down to 6: `hidden` is 0, so a
    // surviving "Show fewer" would be a control that changes nothing.
    renderNow(
      taskState({
        frontier: [
          ...Array.from({ length: 6 }, (_, i) =>
            itemDTO({ id: `q${i}`, title: `Q${i}`, context: "@computer", size: "quick" }),
          ),
          itemDTO({ id: "d1", title: "Deep one", context: "@computer", size: "deep" }),
        ],
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: "Show 1 more in @computer" }));
    expect(screen.getByText("Deep one")).toBeDefined();

    // Filter to `quick`: the column is exactly at the cap now.
    fireEvent.click(screen.getByRole("button", { name: /^Filter/ }));
    fireEvent.click(screen.getByRole("button", { name: "size quick" }));

    expect(screen.queryByText("Show fewer")).toBeNull();
    expect(screen.queryByText(/ more$/)).toBeNull();
  });

  it("states urgency in words as well as colour", () => {
    // ADR-0021 decision 2: colour is never the only carrier, and the words are
    // text rather than `ItemRow`'s `title` tooltip.
    renderNow(taskState({ frontier: [itemDTO({ id: "i1", title: "Renew it", deadline: "1999-01-01" })] }));

    // Once, on the card itself — which is the occurrence that makes colour
    // non-load-bearing. The legend above the board used to say it a second
    // time; it was deleted as chrome that repeated what every card already
    // states.
    expect(screen.getAllByText("Overdue")).toHaveLength(1);
  });

  // `docs/SURFACES.md` records the triage section's `60dvh` cap as the ONLY
  // independent scroll container in the centre column, and ADR-0021 decision 3
  // makes that a live constraint rather than a description: the columns wrap
  // onto more lines instead of scrolling, and no column overflows on its own.
  // jsdom cannot lay out, so this asserts the *declarations* that would make a
  // scroller — which is the half of the criterion a test can hold. The widths
  // themselves are hand-reviewed on a device with real items (#273's
  // disposition, recorded in `docs/SURFACES.md`).
  it("adds no scroll container of its own — the columns wrap instead", () => {
    renderNow(
      taskState({
        frontier: Array.from({ length: 20 }, (_, index) =>
          itemDTO({ id: `i${index}`, title: `Action ${index}`, context: `@c${index % 5}` }),
        ),
      }),
    );

    // Anchored on the columns themselves rather than on a spacing token: the
    // wrap container is the one whose children are the column headings, so an
    // unrelated `gap` change cannot silently make this test vacuous.
    const heading = screen.getByRole("heading", { name: "@c0" });
    const column = heading.closest("div")?.parentElement;
    const wrapper = column?.parentElement;
    expect(wrapper?.style.flexWrap).toBe("wrap");
    // Five columns, all siblings in the one wrap container — no nesting, no
    // per-line sub-container that could scroll on its own.
    expect(wrapper?.childElementCount).toBe(5);

    // And nothing from the wrap container up to the page declares a scroller,
    // which is the assertion that keeps `docs/SURFACES.md`'s "only independent
    // scroll container" clause true. Walking ancestors rather than every div
    // means an element that never sets `overflow` cannot pad the pass count.
    let node: HTMLElement | null = wrapper ?? null;
    let checked = 0;
    while (node && node !== document.body) {
      expect([node.style.overflow, node.style.overflowX, node.style.overflowY]).toEqual(["", "", ""]);
      checked += 1;
      node = node.parentElement;
    }
    expect(checked).toBeGreaterThan(1);
  });

  it("names the blockers holding an item off the frontier", () => {
    renderNow(
      taskState({
        blocked: [
          blockedEntryDTO(itemDTO({ id: "i1", title: "Hang the door" }), [
            itemDTO({ id: "b1", title: "Buy hinges" }),
          ]),
        ],
      }),
    );
    expect(screen.getByText(/Buy hinges/)).toBeDefined();
  });

  it("opens item detail on a frontier row click", () => {
    const { onOpenItem } = renderNow(
      taskState({ frontier: [itemDTO({ id: "i1", title: "Renew the passport" })] }),
    );
    // Two buttons carry this title now — the row and its mark-done
    // checkmark — so the row is the one whose name does NOT start "Mark".
    fireEvent.click(screen.getByRole("button", { name: /^(?!Mark ).*Renew the passport/ }));
    expect(onOpenItem).toHaveBeenCalledWith("i1");
  });

  it("says nothing is startable when both queries are empty", () => {
    renderNow(taskState());
    expect(screen.getByText("Nothing to start")).toBeDefined();
  });
});

describe("NowScreen — the aside (#245, ADR-0015)", () => {
  const ZONE = "America/Los_Angeles";

  function civilDate(dayOffset: number): string {
    return new Intl.DateTimeFormat("en-CA", {
      timeZone: ZONE,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
    }).format(new Date(NOW_MS + dayOffset * 86_400_000));
  }

  it("feeds the ranked region from task state — no context tile, no demo card", () => {
    const day = civilDate(1);
    renderNow(
      taskState({
        bindings: [
          bindingDTO({ key: BINDING_KEY, value: { state: "text", text: "https://example.gov" } }),
        ],
        paneReads: {
          [SOURCE]: paneReadDTO({
            snapshots: [
              paneSnapshotDTO({
                envelope: {
                  kind: "ok",
                  schema: SOURCE,
                  polledEveryMs: 86_400_000,
                  body: wasteBody({ zone: ZONE, scheduled: day, collectedOn: day }),
                },
              }),
            ],
          }),
        },
      }),
    );

    expect(screen.getByText("Trash Tonight")).toBeTruthy();
    // The calendar context tile ADR-0015 replaced is gone entirely.
    expect(screen.queryByText("No calendar connected")).toBeNull();
  });

  it("renders the same region in demo mode, from the demo fixture", () => {
    // `?demo` photographs the REAL shell: same component, different inputs.
    render(
      <NowScreen
        demo={DEMO_DATA}
        onScreen={() => {}}
        task={taskState()}
        nowMs={NOW_MS}
        selectedItemId={null}
        onOpenItem={() => {}}
        onCloseItemDetail={() => {}}
        onAct={() => {}}
        calendarReads={{}}
        calendarConnected={false}
      />,
    );

    expect(screen.getByText("Trash Tonight")).toBeTruthy();
  });

  // #401 / ADR-0021 decision 6. The landmark was called `Context` long after
  // ADR-0015 swapped the calendar context tile out for the ranked region, and
  // the word was needed for the frontier's grouping axis. What matters is not
  // the string but that the landmark still HAS an accessible name at all:
  // `layout.tsx` exists to give it one, because "a complementary landmark with
  // no accessible name is just 'complementary', and there is one on four
  // screens". A rename that fell through to `undefined` would typecheck.
  it("names its complementary landmark for the standing questions it holds", () => {
    renderNow(taskState());

    const aside = screen.getByRole("complementary", { name: "Standing questions" });
    expect(aside.tagName).toBe("ASIDE");
    // The stale name is gone, and — the point of the rename — nothing on the
    // screen calls itself Context in the landmark sense any more, so the axis
    // control #402 adds is free to use the word.
    expect(screen.queryByRole("complementary", { name: "Context" })).toBeNull();
  });
});

describe("NowScreen — the calendar-reads arm (#267/#122)", () => {
  it("threads a delivered calendar read into the weekend pane's own render, not just the store snapshot", () => {
    // The defect this pins: `calendarReads: {}` was hardcoded at the call
    // site, so `CalendarState.eventReads` had zero production readers even
    // though the store leg was real. `CalendarReadProbe` used to be the
    // stand-in consumer that proved delivery; #122 registered the real one
    // (the weekend-plans pane), so this now asserts against ITS render —
    // an event landing on screen through the actual `NowScreen` ->
    // `realQuestionInputs` -> `RankedRegion` -> `weekendQuestion` thread.
    const task = taskState();
    // Anchored to the module's OWN window calculation (never a fixed date
    // string) so the test is timezone-independent: an hour into the
    // current-or-next weekend is unambiguously `live`, which is what keeps
    // the pane expanded by default (`collapse.ts`'s `defaultCollapsed`).
    const testNowMs = weekendWindow(Date.now()).startMs + 60 * 60 * 1000;
    const read: CalendarReadDTO = {
      state: "read",
      events: [
        {
          providerEventId: "evt-1",
          calendarId: "cal-primary",
          title: "Standup",
          when: { kind: "timed", startMs: testNowMs, endMs: testNowMs + 3_600_000 },
          recurrenceId: null,
          location: null,
          organizer: null,
          status: "confirmed",
          providerUpdatedAtMs: testNowMs - 900,
          htmlLink: null,
        },
      ],
      freshness: { kind: "age", ageMs: 60_000, declaredCadenceMs: 900_000 },
    };
    const calendarReads = { [CALENDAR_REQUEST_KEY]: read };

    render(
      <NowScreen
        demo={null}
        onScreen={() => {}}
        task={task}
        nowMs={testNowMs}
        selectedItemId={null}
        onOpenItem={() => {}}
        onCloseItemDetail={() => {}}
        onAct={() => {}}
        calendarReads={calendarReads}
        calendarConnected
      />,
    );

    expect(screen.getByText("Standup")).toBeTruthy();
  });
});

// The row checkmark (mark done from any live stage): what only a mount can
// prove is that the button inside the activatable row completes WITHOUT also
// opening item detail — the row's own click is a selection, and the two
// gestures share one surface.
describe("NowScreen — the mark-done checkmark", () => {
  it("completes a frontier item in one click, without opening its detail", () => {
    const item = itemDTO({ id: "i1", title: "Water the ferns", stage: "ready" });
    const { onAct, onOpenItem } = renderNow(taskState({ frontier: [item] }));

    fireEvent.click(screen.getByRole("button", { name: 'Mark "Water the ferns" done' }));
    expect(onAct).toHaveBeenCalledWith("i1", "complete");
    expect(onOpenItem).not.toHaveBeenCalled();
  });

  it("offers it on a blocked row too — the wait ended because the item was finished", () => {
    const entry = blockedEntryDTO(
      itemDTO({ id: "b1", title: "Fence quote", stage: "blocked" }),
      [itemDTO({ id: "b2", title: "The contractor's callback" })],
    );
    const { onAct } = renderNow(taskState({ blocked: [entry] }));

    fireEvent.click(screen.getByRole("button", { name: 'Mark "Fence quote" done' }));
    expect(onAct).toHaveBeenCalledWith("b1", "complete");
  });

  it("disables the checkmark while the item is pending", () => {
    const item = itemDTO({ id: "i1", title: "Queued thing", stage: "ready", pending: true });
    renderNow(taskState({ frontier: [item] }));
    expect(
      screen.getByRole("button", { name: 'Mark "Queued thing" done' }).hasAttribute("disabled"),
    ).toBe(true);
  });
});

// ADR-0017/#311's converse of `StatusScreen.test.tsx`'s wiring gate: the
// aside is `surface="now"`, so none of the Status-surface questions the
// Status screen owns may ever appear in it, whatever the sort does with
// them. Derived from the registry, not hardcoded — #313-#316 each replace
// one of these labels in turn, and a literal array here would be a shared
// line every one of those four PRs has to edit.
describe("NowScreen — the surface filter (ADR-0017, #311)", () => {
  it("never renders a status-surface pane in Now's aside", () => {
    const statusLabels = QUESTION_ORDER.filter((q) => QUESTIONS[q].surface === "status").map(
      (q) => QUESTIONS[q].label,
    );
    expect(statusLabels.length).toBeGreaterThan(0);

    renderNow(taskState());
    for (const label of statusLabels) {
      expect(screen.queryByText(label)).toBeNull();
    }
    // The `"now"` questions are still there — the filter removes the OTHER
    // surface's panes, not every pane.
    expect(screen.getByRole("button", { name: /which cans/i })).toBeTruthy();
  });
});

// The unsorted captures, in the frontier's own columns. Now used to carry a
// separate triage *section* under the board; the captures are cards among the
// startable actions now, marked with their `triage` stage chip and sorted
// under them. What these cover is the *thread* — which column a capture lands
// in, where in that column it sits, and which editor selecting one opens —
// which is precisely what typecheck cannot see.
describe("NowScreen — the captures in the columns", () => {
  function fakeStorage(seed: Record<string, string> = {}) {
    const entries = { ...seed };
    return {
      entries,
      getItem: (key: string) => entries[key] ?? null,
      setItem: (key: string, value: string) => {
        entries[key] = value;
      },
      removeItem: (key: string) => {
        delete entries[key];
      },
    };
  }

  function renderWithTriage(
    task: TaskState,
    options: {
      selectedItemId?: string | null;
      storage?: ReturnType<typeof fakeStorage>;
    } = {},
  ) {
    const onTriage = vi.fn();
    const onAct = vi.fn();
    const storage = options.storage ?? fakeStorage();
    render(
      <NowScreen
        demo={null}
        onScreen={() => {}}
        task={task}
        nowMs={NOW_MS}
        selectedItemId={options.selectedItemId ?? null}
        onOpenItem={() => {}}
        onCloseItemDetail={() => {}}
        onAct={onAct}
        calendarReads={{}}
        calendarConnected={false}
        onTriage={onTriage}
        storage={storage}
      />,
    );
    return { onTriage, onAct, storage };
  }

  /** `renderWithTriage`'s twin for the one thing it cannot express: changing
   * the selection *without* remounting, which is what closing the slot is. */
  function renderNow(task: TaskState, selectedItemId: string | null) {
    const storage = fakeStorage();
    const screenFor = (selected: string | null) => (
      <NowScreen
        demo={null}
        onScreen={() => {}}
        task={task}
        nowMs={NOW_MS}
        selectedItemId={selected}
        onOpenItem={() => {}}
        onCloseItemDetail={() => {}}
        onAct={() => {}}
        calendarReads={{}}
        calendarConnected={false}
        onTriage={() => {}}
        storage={storage}
      />
    );
    const view = render(screenFor(selectedItemId));
    return { rerender: (next: string | null) => view.rerender(screenFor(next)) };
  }

  const capture = (id: string, title: string, createdAt: number, context: string | null = null) =>
    itemDTO({ id, title, stage: "triage", createdAt, context });

  /** Every item card in the board, in document order. `tabIndex` is what
   * separates them from the surface's other `role="button"` controls (the axis
   * switch, the Filter toggle, the column headers), none of which sets one. */
  const cards = () =>
    screen.getAllByRole("button").filter((node) => node.getAttribute("tabindex") === "0");

  const cardTitles = () => cards().map((node) => node.querySelector("span > span")?.textContent);

  /** The one card for this title. Not `getByRole(… name)`: the card's own
   * mark-done checkmark is a nested button whose accessible name carries the
   * same title, so a name query matches two things. */
  const card = (title: string) => {
    const found = cards().filter((node) => node.textContent?.includes(title));
    expect(found).toHaveLength(1);
    return found[0];
  };

  it("renders a capture as a card in the columns, marked with its stage", () => {
    renderWithTriage(
      taskState({
        frontier: [itemDTO({ id: "i1", title: "Renew the passport", stage: "ready" })],
        triageInbox: [capture("c1", "Ring the plumber", 500)],
      }),
    );

    expect(card("Ring the plumber").textContent).toContain("Triage");
    // And the section that used to hold it is gone entirely — no header, no
    // count, no second place to look for the same inbox.
    expect(screen.queryByText(/unsorted/)).toBeNull();
  });

  it("puts the captures under the startable actions of their column", () => {
    // Both land in `No context`: the action names none, and a capture names
    // nothing at all until somebody triages it. Order within the column is the
    // whole assertion.
    renderWithTriage(
      taskState({
        frontier: [itemDTO({ id: "i1", title: "Renew the passport", stage: "ready" })],
        triageInbox: [capture("c1", "Ring the plumber", 500)],
      }),
    );

    expect(cardTitles()).toEqual(["Renew the passport", "Ring the plumber"]);
  });

  it("groups a capture by the live axis when it already names a value", () => {
    // A capture a sweeper set a context on is not homeless — it belongs in
    // that context's column, which is the point of grouping the captures at
    // all rather than stacking them somewhere separate.
    renderWithTriage(
      taskState({
        frontier: [
          itemDTO({ id: "i1", title: "Prune the hedge", context: "@garden", stage: "ready" }),
        ],
        triageInbox: [capture("c1", "Buy secateurs", 500, "@garden")],
      }),
    );

    const headings = columnHeadings().map((node) => node.textContent);
    expect(headings).toEqual(["@garden"]);
    expect(cardTitles()).toEqual(["Prune the hedge", "Buy secateurs"]);
  });

  it("orders the captures oldest first among themselves, the Triage screen's own rule", () => {
    renderWithTriage(
      taskState({
        triageInbox: [capture("c2", "Newer capture", 900), capture("c1", "Older capture", 100)],
      }),
    );
    expect(cardTitles()).toEqual(["Older capture", "Newer capture"]);
  });

  it("renders the board for an inbox with nothing promoted yet", () => {
    // The commonest state of a new device: captures swept in, nothing triaged.
    // "Nothing to start" would be a lie about a screen that is saying exactly
    // what to do next.
    renderWithTriage(taskState({ triageInbox: [capture("c1", "Ring the plumber", 500)] }));
    expect(screen.queryByText("Nothing to start")).toBeNull();
    expect(card("Ring the plumber")).toBeDefined();
  });

  it("still says nothing to start when there is genuinely nothing at all", () => {
    renderWithTriage(taskState());
    expect(screen.getByText("Nothing to start")).toBeDefined();
  });

  it("opens the triage editor above the columns, which stay standing", () => {
    renderWithTriage(
      taskState({
        frontier: [itemDTO({ id: "i1", title: "Renew the passport", stage: "ready" })],
        triageInbox: [capture("c1", "Ring the plumber", 500)],
      }),
      { selectedItemId: "c1" },
    );

    // The editor is `TriageRow`'s, never `ItemDetailPanel`'s — a capture has no
    // act vocabulary, so the detail panel would offer it nothing.
    expect(screen.getByLabelText("Title")).toBeDefined();
    expect(screen.queryByRole("button", { name: /^Start$/ })).toBeNull();
    // ADR-0021 decision 7, now covering captures: the alternatives stay visible.
    expect(card("Renew the passport")).toBeDefined();
    expect(screen.getByRole("heading", { level: 2, name: "No context" })).toBeDefined();
  });

  it("keeps the captures on the board while a startable action is open", () => {
    // The section this replaced was withheld whenever item detail opened,
    // because two editors would have been open at once. One slot holds one
    // editor now, so a capture's *card* has no reason to go anywhere.
    renderWithTriage(
      taskState({
        frontier: [itemDTO({ id: "i1", title: "Renew the passport", stage: "ready" })],
        triageInbox: [capture("c1", "Ring the plumber", 500)],
      }),
      { selectedItemId: "i1" },
    );

    expect(screen.getByRole("heading", { name: "Renew the passport" })).toBeDefined();
    expect(card("Ring the plumber")).toBeDefined();
    // No triage editor: the slot is the detail panel's.
    expect(screen.queryByLabelText("Title")).toBeNull();
  });

  it("promotes through the same one-call mutation the Triage screen uses", () => {
    const { onTriage } = renderWithTriage(
      taskState({ triageInbox: [capture("c1", "Ring the plumber", 500)] }),
      { selectedItemId: "c1" },
    );

    fireEvent.change(screen.getByLabelText("Size"), { target: { value: "quick" } });
    fireEvent.click(screen.getByRole("button", { name: /promote to ready/i }));

    expect(onTriage).toHaveBeenCalledTimes(1);
    expect(onTriage.mock.calls[0][0]).toBe("c1");
    expect(onTriage.mock.calls[0][1]).toBe("ready");
    expect(onTriage.mock.calls[0][2]).toMatchObject({ size: "quick" });
  });

  it("finishes a capture from its card, through the one act every screen shares", () => {
    const { onAct } = renderWithTriage(
      taskState({ triageInbox: [capture("c1", "Ring the plumber", 500)] }),
    );

    fireEvent.click(screen.getByRole("button", { name: 'Mark "Ring the plumber" done' }));
    expect(onAct).toHaveBeenCalledWith("c1", "complete");
  });

  // #418. On Triage the rows stay mounted in a list, so a late failure always
  // has its row to land in. Here the row IS the slot: closing it unmounts
  // `TriageRow`, and a result broadcast afterwards had nowhere to go — the
  // capture came back to the board saying nothing had gone wrong. The screen
  // says it instead, and names the item.
  const failedTriage = (itemId: string, error: string | null = "409 conflict") => ({
    seed: "s1",
    itemId,
    kind: "failed" as const,
    error,
  });

  it("states a failed triage above the columns when no row is open to wear it", () => {
    renderWithTriage(
      taskState({
        triageInbox: [capture("c1", "Ring the plumber", 500)],
        lastTriage: failedTriage("c1"),
      }),
    );

    const alerts = screen.getAllByRole("alert");
    expect(alerts).toHaveLength(1);
    expect(alerts[0].textContent).toBe('Triage didn\'t apply to "Ring the plumber" — 409 conflict');
    // The capture is back on the board, which is the state the alert explains.
    expect(card("Ring the plumber")).toBeDefined();
  });

  it("states it while a DIFFERENT capture is open, keeping the failure with its own item", () => {
    renderWithTriage(
      taskState({
        triageInbox: [capture("c1", "Ring the plumber", 500), capture("c2", "Book the MOT", 400)],
        lastTriage: failedTriage("c1"),
      }),
      { selectedItemId: "c2" },
    );

    const alerts = screen.getAllByRole("alert");
    expect(alerts).toHaveLength(1);
    expect(alerts[0].textContent).toContain("Ring the plumber");
  });

  it("does not double the failure onto the screen while the failing capture is open", () => {
    renderWithTriage(
      taskState({
        triageInbox: [capture("c1", "Ring the plumber", 500)],
        lastTriage: failedTriage("c1"),
      }),
      { selectedItemId: "c1" },
    );

    // `TriageRow`'s own paragraph, and only that one.
    const alerts = screen.getAllByRole("alert");
    expect(alerts).toHaveLength(1);
    expect(alerts[0].textContent).toBe("409 conflict");
  });

  it("survives the slot closing — the failure outlives the row that issued it", () => {
    const task = taskState({
      triageInbox: [capture("c1", "Ring the plumber", 500)],
      lastTriage: failedTriage("c1"),
    });
    const { rerender } = renderNow(task, "c1");
    expect(screen.getByRole("alert").textContent).toBe("409 conflict");

    // The reader closes the panel. The result is still the last thing that
    // happened, so it must still be on screen.
    rerender(null);
    expect(screen.getByRole("alert").textContent).toBe(
      'Triage didn\'t apply to "Ring the plumber" — 409 conflict',
    );
  });

  it("says nothing about a triage that worked", () => {
    renderWithTriage(
      taskState({
        triageInbox: [capture("c1", "Ring the plumber", 500)],
        lastTriage: { seed: "s1", itemId: "c1", kind: "ok", error: null },
      }),
    );

    expect(screen.queryByRole("alert")).toBeNull();
  });

  // #418's twin. `ItemDetailPanel`'s `actError` renders only while the panel is
  // open, so an act that failed after the reader closed it was displayed
  // nowhere — the same defect on the other mutation, and the one this session
  // fixed alongside the general question of whether the store should hold more
  // than one failure at a time (it holds one per KIND, which is what these two
  // lines are honest about).
  const failedAct = (itemId: string, error: string | null = "409 conflict") => ({
    seed: "s2",
    itemId,
    action: "complete" as const,
    kind: "failed" as const,
    error,
  });

  it("states a failed act above the columns when no panel is open to wear it", () => {
    renderWithTriage(
      taskState({
        frontier: [itemDTO({ id: "i1", title: "Renew the passport", stage: "ready" })],
        lastAct: failedAct("i1"),
      }),
    );

    const alerts = screen.getAllByRole("alert");
    expect(alerts).toHaveLength(1);
    expect(alerts[0].textContent).toBe(
      'That action didn\'t apply to "Renew the passport" — 409 conflict',
    );
  });

  it("does not double the failure while the failing item's panel is open", () => {
    renderWithTriage(
      taskState({
        frontier: [itemDTO({ id: "i1", title: "Renew the passport", stage: "ready" })],
        lastAct: failedAct("i1"),
      }),
      { selectedItemId: "i1" },
    );

    // `ItemDetailPanel`'s own paragraph, and only that one.
    const alerts = screen.getAllByRole("alert");
    expect(alerts).toHaveLength(1);
    expect(alerts[0].textContent).toBe("409 conflict");
  });

  it("survives the panel closing — the failure outlives the panel that issued it", () => {
    const task = taskState({
      frontier: [itemDTO({ id: "i1", title: "Renew the passport", stage: "ready" })],
      lastAct: failedAct("i1"),
    });
    const { rerender } = renderNow(task, "i1");
    expect(screen.getByRole("alert").textContent).toBe("409 conflict");

    rerender(null);
    expect(screen.getByRole("alert").textContent).toBe(
      'That action didn\'t apply to "Renew the passport" — 409 conflict',
    );
  });

  it("speaks for a failed act on the OPEN capture, whose row says nothing about acts", () => {
    // A capture in the slot gets `TriageRow`, not `ItemDetailPanel` — and
    // `TriageRow`'s checkmark issues an act. Nothing there renders an act
    // failure, so suppressing this line for the open capture would strand it.
    renderWithTriage(
      taskState({
        triageInbox: [capture("c1", "Ring the plumber", 500)],
        lastAct: failedAct("c1"),
      }),
      { selectedItemId: "c1" },
    );

    const alerts = screen.getAllByRole("alert");
    expect(alerts).toHaveLength(1);
    expect(alerts[0].textContent).toBe(
      'That action didn\'t apply to "Ring the plumber" — 409 conflict',
    );
  });

  it("states both failures at once — they are separate results, not one slot", () => {
    renderWithTriage(
      taskState({
        frontier: [itemDTO({ id: "i1", title: "Renew the passport", stage: "ready" })],
        triageInbox: [capture("c1", "Ring the plumber", 500)],
        lastTriage: failedTriage("c1"),
        lastAct: failedAct("i1"),
      }),
    );

    const alerts = screen.getAllByRole("alert");
    expect(alerts).toHaveLength(2);
    expect(alerts[0].textContent).toContain("Ring the plumber");
    expect(alerts[1].textContent).toContain("Renew the passport");
  });

  it("says nothing about an act that worked", () => {
    renderWithTriage(
      taskState({
        frontier: [itemDTO({ id: "i1", title: "Renew the passport", stage: "ready" })],
        lastAct: { seed: "s2", itemId: "i1", action: "complete", kind: "ok", error: null },
      }),
    );

    expect(screen.queryByRole("alert")).toBeNull();
  });
});

// #403's controls, tested through the mounted screen because what they are
// about is the *thread*: the axis and the collapsed set are read from and
// written to the one `storage` prop `NowScreen` resolves, and the clearing rule
// couples two pieces of state that live in different modules. The preference
// modules' own five-test templates are in `frontier-prefs.test.ts`; these are
// the wiring those templates cannot see.
describe("NowScreen — the frontier's controls (#403)", () => {
  function fakeStorage(seed: Record<string, string> = {}) {
    const entries = { ...seed };
    return {
      entries,
      getItem: (key: string) => entries[key] ?? null,
      setItem: (key: string, value: string) => {
        entries[key] = value;
      },
      removeItem: (key: string) => {
        delete entries[key];
      },
    };
  }

  function renderWithStorage(task: TaskState, storage = fakeStorage()) {
    const view = render(
      <NowScreen
        demo={null}
        onScreen={() => {}}
        task={task}
        nowMs={NOW_MS}
        selectedItemId={null}
        onOpenItem={() => {}}
        onCloseItemDetail={() => {}}
        onAct={() => {}}
        calendarReads={{}}
        calendarConnected={false}
        storage={storage}
      />,
    );
    return { storage, unmount: view.unmount };
  }

  const spread = () =>
    taskState({
      frontier: [
        itemDTO({ id: "i1", title: "Email the council", context: "@computer", size: "quick", energy: "low" }),
        itemDTO({ id: "i2", title: "Prune the hedge", context: "@garden", size: "deep", energy: "high" }),
        itemDTO({ id: "i3", title: "Ring the vet", context: "@phone", size: "quick", energy: "low" }),
      ],
    });

  it("collapses a column in place, keeping its heading and its count readable", () => {
    renderWithStorage(spread());

    const header = screen.getByRole("button", { expanded: true, name: /@computer/ });
    fireEvent.click(header);

    // Shut: the card is gone, but the column still says what it is and how
    // much is inside it — a closed column that hid its own count would be
    // worse than no count at all. The count is a sibling of the heading rather
    // than inside it, so neither accessible name reads "@computer 1".
    expect(screen.queryByText("Email the council")).toBeNull();
    const shut = screen.getByRole("button", { expanded: false, name: "@computer" });
    const shutHeader = shut.closest("div");
    expect(shutHeader?.textContent).toContain("@computer");
    expect(shutHeader?.textContent).toContain("1");
    // Its neighbours are untouched — collapse is per-column and additive.
    expect(screen.getByText("Prune the hedge")).toBeDefined();

    fireEvent.click(shut);
    expect(screen.getByText("Email the council")).toBeDefined();
  });

  it("persists the collapsed set and the axis across a remount", () => {
    const storage = fakeStorage();
    const first = renderWithStorage(spread(), storage);

    fireEvent.click(screen.getByRole("button", { name: "Size" }));
    fireEvent.click(screen.getByRole("button", { expanded: true, name: /quick/ }));
    first.unmount();

    // A reload: same storage, fresh mount.
    renderWithStorage(spread(), storage);
    expect(screen.getByRole("button", { name: "Size", pressed: true })).toBeDefined();
    expect(screen.getByRole("button", { expanded: false, name: /quick/ })).toBeDefined();
    expect(screen.queryByText("Email the council")).toBeNull();
  });

  it("clears the collapsed set when the axis changes — the labels no longer exist", () => {
    const { storage } = renderWithStorage(spread());

    fireEvent.click(screen.getByRole("button", { expanded: true, name: /@computer/ }));
    expect(storage.entries["hb.now.frontier-collapsed"]).toBe('["@computer"]');

    fireEvent.click(screen.getByRole("button", { name: "Energy" }));

    // Nothing shut, and the stored key is gone rather than holding a label from
    // an axis that is no longer live. Scoped to the column headings, since the
    // Filter button legitimately carries its own `aria-expanded={false}`.
    expect("hb.now.frontier-collapsed" in storage.entries).toBe(false);
    // Energy across this spread is low, high, low — two columns.
    const headers = columnHeadings().map((heading) => heading.querySelector("button"));
    expect(headers).toHaveLength(2);
    for (const header of headers) {
      expect(header?.getAttribute("aria-expanded")).toBe("true");
    }
  });

  it("prunes a dead column's collapse rather than keeping it forever", () => {
    // ADR-0021 decision 5 rejects the `settings` table because "an override map
    // would accrete keys for panes that no longer exist". Device-local storage
    // has the same failure, and `questions/collapse.ts` prunes for exactly this
    // reason. Without it, a column of that name later would come back collapsed.
    const storage = fakeStorage();
    const first = renderWithStorage(spread(), storage);

    fireEvent.click(screen.getByRole("button", { expanded: true, name: "@garden" }));
    fireEvent.click(screen.getByRole("button", { expanded: true, name: "@phone" }));
    expect(JSON.parse(storage.entries["hb.now.frontier-collapsed"])).toEqual([
      "@garden",
      "@phone",
    ]);
    first.unmount();

    // The @garden item is done and gone; that column no longer exists.
    const shrunk = taskState({
      frontier: [itemDTO({ id: "i1", title: "Email the council", context: "@computer" })],
    });
    renderWithStorage(shrunk, storage);
    fireEvent.click(screen.getByRole("button", { expanded: true, name: "@computer" }));

    // The next write carries only keys a column could still have.
    expect(JSON.parse(storage.entries["hb.now.frontier-collapsed"])).toEqual(["@computer"]);
  });

  it("does not prune a column the live filter is merely hiding", () => {
    // The subtlety in the pruning: a filtered-out column is not a dead one, and
    // forgetting it was shut would be a silent loss.
    const storage = fakeStorage();
    renderWithStorage(spread(), storage);

    fireEvent.click(screen.getByRole("button", { expanded: true, name: "@garden" }));

    // Filter to quick, which hides @garden's only (deep) item entirely.
    fireEvent.click(screen.getByRole("button", { name: /^Filter/ }));
    fireEvent.click(screen.getByRole("button", { name: "size quick" }));
    expect(screen.queryByRole("button", { name: "@garden" })).toBeNull();

    // Toggling a surviving column writes — and must not drop @garden.
    fireEvent.click(screen.getByRole("button", { expanded: true, name: "@computer" }));
    expect(JSON.parse(storage.entries["hb.now.frontier-collapsed"]).sort()).toEqual([
      "@computer",
      "@garden",
    ]);
  });

  it("clears the per-column reveal state when the axis changes, not just the collapse", () => {
    renderWithStorage(
      taskState({
        frontier: Array.from({ length: 8 }, (_, i) =>
          itemDTO({ id: `i${i}`, title: `Action ${i}`, context: "@computer", size: "quick" }),
        ),
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: "Show 2 more in @computer" }));
    expect(screen.getByText("Action 7")).toBeDefined();

    fireEvent.click(screen.getByRole("button", { name: "Size" }));
    // Same eight items, now one "quick" column — re-collapsed to the cap,
    // because the expansion was keyed by a label that no longer exists.
    expect(screen.queryByText("Action 7")).toBeNull();
    expect(screen.getByRole("button", { name: "Show 2 more in quick" })).toBeDefined();
  });

  // Neither the collapsed state nor the control belongs to this screen any
  // more: one button in the shell's header owns both directions, and `App.tsx`
  // owns the state. What is left to pin here is that the prop is obeyed.
  // Persistence lives in `questions/aside-prefs.test.ts`, the button in
  // `shell/Header.test.tsx`.
  it("renders the standing-questions aside open by default", () => {
    renderWithStorage(spread());

    // Open unless told otherwise — a question that has fired is the one thing
    // on Now you did not ask for and must not be hidden by an unset preference.
    expect(screen.getByRole("complementary", { name: "Standing questions" })).toBeDefined();
  });

  it("drops the landmark entirely when the aside is shut, rather than leaving an empty one", () => {
    render(
      <NowScreen
        demo={null}
        onScreen={() => {}}
        task={spread()}
        nowMs={NOW_MS}
        selectedItemId={null}
        onOpenItem={() => {}}
        onCloseItemDetail={() => {}}
        onAct={() => {}}
        calendarReads={{}}
        calendarConnected={false}
        storage={fakeStorage()}
        asideCollapsed
      />,
    );

    // No strip is left behind: an `aside` named "Standing questions" holding
    // nothing is a landmark that lies, and the reopen control is the header's.
    expect(screen.queryByRole("complementary", { name: "Standing questions" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Standing questions" })).toBeNull();
  });

  it("does not persist the filter selection — Now never opens filtered", () => {
    const storage = fakeStorage();
    const first = renderWithStorage(spread(), storage);

    fireEvent.click(screen.getByRole("button", { name: /^Filter/ }));
    fireEvent.click(screen.getByRole("button", { name: "size deep" }));
    expect(screen.queryByText("Email the council")).toBeNull();
    first.unmount();

    renderWithStorage(spread(), storage);
    // Opening Now to a filtered board you would misread as an empty frontier
    // is the failure this avoids.
    expect(screen.getByText("Email the council")).toBeDefined();
    expect(screen.getByText("Prune the hedge")).toBeDefined();
  });

  it("narrows behind the Filter button, with an honest count and an active badge", () => {
    renderWithStorage(spread());

    // Shut by default: filtering is the occasional gesture, so it costs one
    // button rather than four permanent chip rows.
    expect(screen.queryByRole("button", { name: "size deep" })).toBeNull();
    expect(screen.queryByText(/of 3 shown/)).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /^Filter/ }));
    fireEvent.click(screen.getByRole("button", { name: "size deep" }));

    // One of the three is deep, and the readout says so rather than leaving the
    // reader to notice two cards missing.
    expect(screen.getByText("1 of 3 shown")).toBeDefined();
    expect(screen.getByText("Prune the hedge")).toBeDefined();
    expect(screen.queryByText("Email the council")).toBeNull();

    // OR *within* a facet widens: deep plus quick is every judged item.
    fireEvent.click(screen.getByRole("button", { name: "size quick" }));
    expect(screen.getByText("3 of 3 shown")).toBeDefined();

    // AND *across* facets narrows again: of those three, only one is @garden.
    fireEvent.click(screen.getByRole("button", { name: "context @garden" }));
    expect(screen.getByText("1 of 3 shown")).toBeDefined();
    expect(screen.getByText("Prune the hedge")).toBeDefined();

    // The count follows the panel shut, and the button carries the tally of
    // picked values (deep, quick, @garden).
    fireEvent.click(screen.getByRole("button", { name: /^Filter/ }));
    expect(screen.getByText("1 of 3 shown")).toBeDefined();
    expect(screen.getByRole("button", { name: /^Filter/ }).textContent).toContain("3");
  });

  it("says an empty result is empty, not that the frontier is", () => {
    renderWithStorage(spread());

    fireEvent.click(screen.getByRole("button", { name: /^Filter/ }));
    // Nothing is both quick and high-energy in this spread.
    fireEvent.click(screen.getByRole("button", { name: "size quick" }));
    fireEvent.click(screen.getByRole("button", { name: "energy high" }));

    expect(screen.getByText("Nothing matches")).toBeDefined();
    expect(screen.getByText("0 of 3 shown")).toBeDefined();
    // The two facts must not look alike.
    expect(screen.queryByText("Nothing to start")).toBeNull();
  });

  it("applies a preference for the session even when storage cannot keep it", () => {
    // Reads and writes that go nowhere: the preference still applies until the
    // tab closes. `frontier-prefs.test.ts` pins the module's own tolerance;
    // this pins that the screen does not depend on a write succeeding.
    const readOnly = {
      entries: {} as Record<string, string>,
      getItem: (key: string) => readOnly.entries[key] ?? null,
      setItem: () => {
        throw new Error("nope");
      },
      removeItem: () => {
        throw new Error("nope");
      },
    };
    renderWithStorage(spread(), readOnly);

    fireEvent.click(screen.getByRole("button", { name: "Size" }));
    expect(screen.getByRole("heading", { name: "quick" })).toBeDefined();
    fireEvent.click(screen.getByRole("button", { expanded: true, name: "quick" }));
    expect(screen.queryByText("Email the council")).toBeNull();
  });
});

// #404: selection stops being a takeover. Every one of these is about the
// *thread* — the panel and the columns are separate subtrees now, and what
// typecheck cannot see is whether both are mounted at once, whether the card
// still says it is the source, and whether the optimistic fallback PR #207
// bought still holds when the item has left every live query.
describe("NowScreen — selection above the columns (#404)", () => {
  const spread = () =>
    taskState({
      frontier: [
        itemDTO({ id: "i1", title: "Email the council", context: "@computer", stage: "ready" }),
        itemDTO({ id: "i2", title: "Prune the hedge", context: "@garden", stage: "ready" }),
      ],
    });

  it("mounts the panel with the columns still mounted and visible", () => {
    renderNow(spread(), "i1");

    // The panel: `ItemDetailPanel` heads with the item's title.
    expect(screen.getByRole("heading", { name: "Email the council" })).toBeDefined();
    // ...and the columns are still there under it, including the OTHER column,
    // which is the whole point: picking one action must not cost the view of
    // everything you might have picked instead.
    expect(screen.getByRole("heading", { name: "@computer" })).toBeDefined();
    expect(screen.getByRole("heading", { name: "@garden" })).toBeDefined();
    expect(screen.getByText("Prune the hedge")).toBeDefined();
    // The axis switch survives too — the surface is not replaced.
    expect(screen.getByRole("button", { name: "Context", pressed: true })).toBeDefined();
  });

  it("puts the panel above the columns, not below them", () => {
    renderNow(spread(), "i1");

    const headings = screen.getAllByRole("heading").map((node) => node.textContent);
    // "it goes to the top" has to be true of the document order, not just of a
    // CSS position.
    expect(headings.indexOf("Email the council")).toBeLessThan(headings.indexOf("@computer"));
  });

  it("marks the originating card, visibly and programmatically", () => {
    renderNow(spread(), "i1");

    // Two things carry the title now — the panel heading and the source card.
    // The card is the button.
    const card = screen
      .getAllByRole("button")
      .find((node) => node.getAttribute("aria-current") === "true");
    expect(card).toBeDefined();
    expect(card?.textContent).toContain("Email the council");
    // Visibly, not only programmatically. The mark is the accent BORDER, not a
    // fill — the design system gives the answering card "an ember-tinted
    // border, not a fill", so a fill here would be this surface disagreeing
    // with every other card in the app.
    expect(card?.style.border).toBe("1px solid var(--accent-quiet-border)");
    expect(card?.style.background).not.toBe("var(--accent-quiet)");
    // And only the one card.
    expect(
      screen.getAllByRole("button").filter((n) => n.getAttribute("aria-current") === "true"),
    ).toHaveLength(1);
  });

  it("keeps the ranked-region aside mounted while the panel is open", () => {
    // #359 calls this "the one thing this surface has that Triage does not",
    // so it is asserted rather than eyeballed.
    renderNow(spread(), "i1");
    expect(screen.getByRole("complementary", { name: "Standing questions" })).toBeDefined();
  });

  it("keeps the optimistic post-act fallback: the panel survives an item leaving every live query", () => {
    // PR #207's machinery, which the prototype deliberately skipped and
    // production must not. `block` sets a stage neither `frontier` nor
    // `blocked` reads, so without the fallback the panel goes blank.
    const { onAct, rerender } = renderNow(spread(), "i1");

    fireEvent.click(screen.getByRole("button", { name: /mark blocked/i }));
    expect(onAct).toHaveBeenCalledWith("i1", "block");

    rerender(taskState({ frontier: [], blocked: [], pending: {} }), "i1");
    expect(screen.getByRole("heading", { name: "Email the council" })).toBeDefined();
    expect(screen.getByRole("button", { name: /^start$/i }).hasAttribute("disabled")).toBe(true);

    // ...and it re-enables once the queued mutation drains, exactly as before.
    rerender(taskState({ frontier: [], blocked: [], pending: { i1: true } }), "i1");
    rerender(taskState({ frontier: [], blocked: [], pending: { i1: false } }), "i1");
    expect(screen.getByRole("button", { name: /^start$/i }).hasAttribute("disabled")).toBe(false);
  });

  it("returns to the columns on close, with the axis, collapse and filter state intact", () => {
    function fakeStorage(seed: Record<string, string> = {}) {
      const entries = { ...seed };
      return {
        entries,
        getItem: (key: string) => entries[key] ?? null,
        setItem: (key: string, value: string) => {
          entries[key] = value;
        },
        removeItem: (key: string) => {
          delete entries[key];
        },
      };
    }
    const storage = fakeStorage();
    const view = render(
      <NowScreen
        demo={null}
        onScreen={() => {}}
        task={spread()}
        nowMs={NOW_MS}
        selectedItemId={null}
        onOpenItem={() => {}}
        onCloseItemDetail={() => {}}
        onAct={() => {}}
        calendarReads={{}}
        calendarConnected={false}
        storage={storage}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Project" }));
    fireEvent.click(screen.getByRole("button", { expanded: true, name: "No project" }));

    // The filter is the third thing #404 names, and unlike the other two it is
    // deliberately NOT persisted (#403) — it survives only because it is state
    // in a component that must not remount. That is exactly why it is asserted
    // here: a remount would lose it silently while the axis and collapse, being
    // restored from storage, would still look right.
    fireEvent.click(screen.getByRole("button", { name: "Filter" }));
    fireEvent.click(screen.getByRole("button", { name: "context @garden" }));
    // Asserted through the count rather than the hidden card, because the
    // collapsed column above already hides every card either way.
    expect(screen.getByText("1 of 2 shown")).toBeDefined();

    const withSelection = (selected: string | null) => (
      <NowScreen
        demo={null}
        onScreen={() => {}}
        task={spread()}
        nowMs={NOW_MS}
        selectedItemId={selected}
        onOpenItem={() => {}}
        onCloseItemDetail={() => {}}
        onAct={() => {}}
        calendarReads={{}}
        calendarConnected={false}
        storage={storage}
      />
    );

    view.rerender(withSelection("i1"));
    // Open: the axis, the shut column and the picked facet are all what they were.
    expect(screen.getByRole("button", { name: "Project", pressed: true })).toBeDefined();
    expect(screen.getByRole("button", { expanded: false, name: "No project" })).toBeDefined();
    expect(screen.getByRole("button", { name: "context @garden", pressed: true })).toBeDefined();
    expect(screen.getByText("1 of 2 shown")).toBeDefined();

    view.rerender(withSelection(null));
    expect(screen.queryByRole("heading", { name: "Email the council" })).toBeNull();
    expect(screen.getByRole("button", { name: "Project", pressed: true })).toBeDefined();
    expect(screen.getByRole("button", { expanded: false, name: "No project" })).toBeDefined();
    expect(screen.getByRole("button", { name: "context @garden", pressed: true })).toBeDefined();
    expect(screen.getByText("1 of 2 shown")).toBeDefined();
  });

  it("withholds \"Nothing to start\" while the panel is open", () => {
    // Reachable, not hypothetical: block your only startable item and both
    // queries go empty while the optimistic fallback keeps the panel standing.
    // Without the guard the screen showed the open item, "Nothing to start", and
    // no triage section — the combination `NowScreen`'s own comment warns
    // against, since triage is withheld whenever the panel is open.
    const { rerender } = renderNow(
      taskState({
        frontier: [itemDTO({ id: "i1", title: "Email the council", stage: "ready" })],
        triageInbox: [itemDTO({ id: "c1", title: "Ring the plumber", stage: "triage" })],
      }),
      "i1",
    );

    fireEvent.click(screen.getByRole("button", { name: /mark blocked/i }));
    rerender(taskState({ frontier: [], blocked: [], pending: {} }), "i1");

    // The panel is still standing on the optimistic item...
    expect(screen.getByRole("heading", { name: "Email the council" })).toBeDefined();
    // ...and neither of the two things that would contradict it is on screen.
    expect(screen.queryByText("Nothing to start")).toBeNull();
    expect(screen.queryByText("Ring the plumber")).toBeNull();

    // Closed, with the frontier genuinely empty, it says so again.
    rerender(taskState({ frontier: [], blocked: [], pending: {} }), null);
    expect(screen.getByText("Nothing to start")).toBeDefined();
  });

  it("brings the panel into view when it opens", () => {
    const scrollIntoView = vi.fn();
    const original = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = scrollIntoView;
    try {
      const { rerender } = renderNow(spread(), null);
      expect(scrollIntoView).not.toHaveBeenCalled();

      rerender(spread(), "i1");
      // A card near the bottom of a long board would otherwise expand
      // off-screen, which makes "it goes to the top" true of the DOM and false
      // for the reader.
      expect(scrollIntoView).toHaveBeenCalledTimes(1);

      // A different item is a new selection, so it scrolls again.
      rerender(spread(), "i2");
      expect(scrollIntoView).toHaveBeenCalledTimes(2);
    } finally {
      Element.prototype.scrollIntoView = original;
    }
  });
});
