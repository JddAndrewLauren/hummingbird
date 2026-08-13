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

function renderNow(task: TaskState, selectedItemId: string | null = null) {
  const onAct = vi.fn();
  const onOpenItem = vi.fn();
  const onCloseItemDetail = vi.fn();
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
      />,
    );
  return { onAct, onOpenItem, onCloseItemDetail, rerender };
}

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

    // Six of nine on screen, and the count never lies about the rest.
    expect(screen.getByRole("heading", { name: "@computer" })).toBeDefined();
    expect(screen.queryByText("Action 5")).toBeDefined();
    expect(screen.queryByText("Action 6")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "3 more" }));
    expect(screen.getByText("Action 8")).toBeDefined();

    fireEvent.click(screen.getByRole("button", { name: "Show fewer" }));
    expect(screen.queryByText("Action 6")).toBeNull();
  });

  it("states urgency in words as well as colour", () => {
    // ADR-0021 decision 2: colour is never the only carrier, and the words are
    // text rather than `ItemRow`'s `title` tooltip.
    renderNow(taskState({ frontier: [itemDTO({ id: "i1", title: "Renew it", deadline: "1999-01-01" })] }));

    // Twice on purpose: once naming the swatch in the legend, once on the card
    // itself. The card's is the one that makes colour non-load-bearing.
    expect(screen.getAllByText("Overdue")).toHaveLength(2);
    // `calm` has no swatch, so it is named on cards but never in the legend.
    expect(screen.queryByText("Calm")).toBeNull();
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

    const container = document.body;
    const wrappers = [...container.querySelectorAll<HTMLElement>("div")].filter(
      (node) => node.style.flexWrap === "wrap" && node.style.gap === "var(--space-6)",
    );
    expect(wrappers).toHaveLength(1);
    for (const node of container.querySelectorAll<HTMLElement>("div")) {
      expect(node.style.overflowX).toBe("");
      expect(node.style.overflow).toBe("");
    }
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

// Now's triage section: the same inbox the Triage screen renders, brought
// under the frontier so a capture can be sorted without leaving the screen
// you work from. What these cover is the *thread* — the section is rendered
// from `RealFrontier`, so every one of these is about where it sits and when
// it is there at all, which is precisely what typecheck cannot see.
describe("NowScreen — the triage section", () => {
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
    const onCompleteTriage = vi.fn();
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
        onAct={() => {}}
        calendarReads={{}}
        calendarConnected={false}
        onTriage={onTriage}
        onCompleteTriage={onCompleteTriage}
        storage={storage}
      />,
    );
    return { onTriage, onCompleteTriage, storage };
  }

  const capture = (id: string, title: string, createdAt: number) =>
    itemDTO({ id, title, stage: "triage", createdAt });

  it("renders the inbox under the promoted items, never above them", () => {
    renderWithTriage(
      taskState({
        frontier: [itemDTO({ id: "i1", title: "Renew the passport", stage: "ready" })],
        projects: [projectDTO({ id: "p1", name: "Household" })],
        triageInbox: [capture("c1", "Ring the plumber", 500)],
      }),
    );

    // The frontier's column heading, on the default `context` axis — the
    // section title this used to look for ("No project") is gone with the
    // project grouping (#402), but what it was really asserting is unchanged:
    // triage sits below whatever the frontier rendered.
    const headings = screen.getAllByRole("heading", { level: 2 }).map((node) => node.textContent);
    expect(headings).toContain("No context");
    const triageIndex = headings.findIndex((text) => text?.startsWith("Triage"));
    expect(triageIndex).toBeGreaterThan(headings.indexOf("No context"));
    expect(screen.getByText("1 unsorted")).toBeDefined();
    expect(screen.getByText("Ring the plumber")).toBeDefined();
  });

  it("orders the captures oldest first, the same rule the Triage screen uses", () => {
    renderWithTriage(
      taskState({
        triageInbox: [capture("c2", "Newer capture", 900), capture("c1", "Older capture", 100)],
      }),
    );
    const titles = screen
      .getAllByText(/capture$/)
      .map((node) => node.textContent);
    expect(titles).toEqual(["Older capture", "Newer capture"]);
  });

  it("is absent entirely when the inbox is empty — an empty inbox is good news, not a card", () => {
    renderWithTriage(taskState({ frontier: [itemDTO({ id: "i1", stage: "ready" })] }));
    expect(screen.queryByText(/unsorted/)).toBeNull();
  });

  it("still renders when nothing is promoted yet, beside the honest empty state", () => {
    // The commonest state of a new device: captures swept in, nothing
    // triaged. The early return this replaced showed "Nothing to start" and
    // hid the one thing worth doing.
    renderWithTriage(taskState({ triageInbox: [capture("c1", "Ring the plumber", 500)] }));
    expect(screen.getByText("Nothing to start")).toBeDefined();
    expect(screen.getByText("Ring the plumber")).toBeDefined();
  });

  it("gives way to the item detail panel, so two editors are never open at once", () => {
    renderWithTriage(
      taskState({
        frontier: [itemDTO({ id: "i1", title: "Renew the passport", stage: "ready" })],
        triageInbox: [capture("c1", "Ring the plumber", 500)],
      }),
      { selectedItemId: "i1" },
    );
    expect(screen.getByRole("heading", { name: "Renew the passport" })).toBeDefined();
    expect(screen.queryByText(/unsorted/)).toBeNull();
  });

  it("starts expanded, collapses on the header, and persists that to the injected storage", () => {
    const storage = fakeStorage();
    renderWithTriage(taskState({ triageInbox: [capture("c1", "Ring the plumber", 500)] }), {
      storage,
    });

    // Anchored: every row carries a "Triage" stage badge inside its own
    // toggle button, so a bare /Triage/ matches the whole section.
    const header = screen.getByRole("button", { name: /^Triage \d+ unsorted$/ });
    expect(header.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByText("Ring the plumber")).toBeDefined();

    fireEvent.click(header);
    expect(header.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByText("Ring the plumber")).toBeNull();
    // The count stays readable while collapsed — that is the whole point of
    // capping the list rather than the section.
    expect(screen.getByText("1 unsorted")).toBeDefined();
    expect(storage.entries["hb.now.triage-collapsed"]).toBe("1");
  });

  it("opens collapsed when the device already said so", () => {
    renderWithTriage(taskState({ triageInbox: [capture("c1", "Ring the plumber", 500)] }), {
      storage: fakeStorage({ "hb.now.triage-collapsed": "1" }),
    });
    expect(screen.queryByText("Ring the plumber")).toBeNull();
  });

  it("promotes through the same one-call mutation the Triage screen uses", () => {
    const { onTriage } = renderWithTriage(
      taskState({ triageInbox: [capture("c1", "Ring the plumber", 500)] }),
    );

    // Expanding a row is a selection; the editor is one click away. Anchored
    // on the row's own stage badge, so the mark-done checkmark beside it
    // (named `Mark "Ring the plumber" done`) is never the match.
    fireEvent.click(screen.getByRole("button", { name: /^Triage Ring the plumber/ }));
    fireEvent.change(screen.getByLabelText("Size"), { target: { value: "quick" } });
    fireEvent.click(screen.getByRole("button", { name: /promote to ready/i }));

    expect(onTriage).toHaveBeenCalledTimes(1);
    expect(onTriage.mock.calls[0][0]).toBe("c1");
    expect(onTriage.mock.calls[0][1]).toBe("ready");
    expect(onTriage.mock.calls[0][2]).toMatchObject({ size: "quick" });
  });

  it("keeps one row open at a time", () => {
    renderWithTriage(
      taskState({
        triageInbox: [capture("c1", "Older capture", 100), capture("c2", "Newer capture", 900)],
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: /^Triage Older capture/ }));
    expect(screen.getAllByLabelText("Title")).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: /^Triage Newer capture/ }));
    expect(screen.getAllByLabelText("Title")).toHaveLength(1);
    expect((screen.getByLabelText("Title") as HTMLInputElement).value).toBe("Newer capture");
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
    const headers = screen
      .getAllByRole("heading", { level: 2 })
      .map((heading) => heading.querySelector("button"));
    expect(headers).toHaveLength(2);
    for (const header of headers) {
      expect(header?.getAttribute("aria-expanded")).toBe("true");
    }
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
    // Visibly, not only programmatically: the accent fill, per ADR-0021.
    expect(card?.style.background).toBe("var(--accent-quiet)");
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

  it("returns to the columns on close, with the axis and collapse state intact", () => {
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
    // Open: the axis and the shut column are still what they were.
    expect(screen.getByRole("button", { name: "Project", pressed: true })).toBeDefined();
    expect(screen.getByRole("button", { expanded: false, name: "No project" })).toBeDefined();

    view.rerender(withSelection(null));
    expect(screen.queryByRole("heading", { name: "Email the council" })).toBeNull();
    expect(screen.getByRole("button", { name: "Project", pressed: true })).toBeDefined();
    expect(screen.getByRole("button", { expanded: false, name: "No project" })).toBeDefined();
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
