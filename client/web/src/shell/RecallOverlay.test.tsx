// @vitest-environment jsdom

// **Recall**'s regression suite (#478): what `Core::search`'s unit tests
// cannot reach — whether this component actually renders whatever `rows`
// and `total` it is handed, in the right groups, with the "N more" line,
// and whether an empty query short-circuits to its own state without
// waiting on an answer. Matching/ordering/the cap are `search.rs`'s own
// tests; this file only exercises the render.

import { describe, expect, it, vi } from "vitest";
import { RecallOverlay } from "./RecallOverlay";
import type { RecallRowDTO } from "../store/protocol";
import type { TaskTriageResult } from "../store/store";
import { fireEvent, render, screen } from "../test/component";

function row(overrides: Partial<RecallRowDTO> = {}): RecallRowDTO {
  return {
    id: "item-1",
    seq: 1,
    title: "buy stamps",
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
    archivedAt: null,
    createdAt: 1_000,
    updatedAt: 1_000,
    version: 1,
    pending: false,
    group: "live",
    ...overrides,
  };
}

// A fixed instant this suite's `nowMs` prop always reads — `useSyncWiring`'s
// re-sampled clock in production, never a fresh `Date.now()` taken here (the
// same reason the component itself never calls it during render).
const TEST_NOW = Date.parse("2026-08-15T12:00:00Z");

function renderOverlay(
  options: {
    query?: string;
    rows?: RecallRowDTO[] | null;
    total?: number;
    onTriage?: ReturnType<typeof vi.fn>;
    lastTriage?: TaskTriageResult | null;
  } = {},
) {
  const onQueryChange = vi.fn();
  const onClose = vi.fn();
  const view = render(
    <RecallOverlay
      open
      query={options.query ?? ""}
      onQueryChange={onQueryChange}
      onClose={onClose}
      rows={options.rows ?? null}
      total={options.total ?? 0}
      projects={[]}
      onTriage={options.onTriage}
      lastTriage={options.lastTriage}
      nowMs={TEST_NOW}
    />,
  );
  return { onQueryChange, onClose, view };
}

describe("RecallOverlay", () => {
  it("renders nothing over the closed prop", () => {
    render(
      <RecallOverlay
        open={false}
        query=""
        onQueryChange={vi.fn()}
        onClose={vi.fn()}
        rows={null}
        total={0}
        projects={[]}
        nowMs={TEST_NOW}
      />,
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("an empty or whitespace-only query shows the type-to-search state, not a result list", () => {
    renderOverlay({ query: "   ", rows: [row()], total: 1 });
    expect(screen.getByText("Type to search")).toBeTruthy();
    expect(screen.queryByText("buy stamps")).toBeNull();
  });

  it('a non-empty query with no answer yet reads "Searching…" rather than "nothing matched"', () => {
    renderOverlay({ query: "stamps", rows: null, total: 0 });
    expect(screen.getByText("Searching…")).toBeTruthy();
  });

  it("a non-empty query with zero rows reads nothing matched", () => {
    renderOverlay({ query: "stamps", rows: [], total: 0 });
    expect(screen.getByText("Nothing matched")).toBeTruthy();
  });

  it("renders one row per result, each labelled with its group", () => {
    renderOverlay({
      query: "shared",
      rows: [
        row({ id: "a", title: "live one", group: "live" }),
        row({ id: "b", title: "done one", group: "done" }),
        row({ id: "c", title: "archived one", group: "archived" }),
      ],
      total: 3,
    });

    expect(screen.getByText("live one")).toBeTruthy();
    expect(screen.getByText("done one")).toBeTruthy();
    expect(screen.getByText("archived one")).toBeTruthy();
    expect(screen.getByText("live")).toBeTruthy();
    expect(screen.getByText("done")).toBeTruthy();
    expect(screen.getByText("archived")).toBeTruthy();
  });

  it("preserves the core's own row order rather than re-sorting", () => {
    renderOverlay({
      query: "shared",
      rows: [row({ id: "a", title: "first" }), row({ id: "b", title: "second" })],
      total: 2,
    });

    const titles = screen.getAllByText(/first|second/).map((node) => node.textContent);
    expect(titles).toEqual(["first", "second"]);
  });

  it('shows the "N more" line only when total exceeds the rendered rows, using the core-supplied count', () => {
    renderOverlay({ query: "shared", rows: [row()], total: 1 });
    expect(screen.queryByText(/more matched/)).toBeNull();
  });

  it('the cap line reads the un-capped total, not rows.length', () => {
    renderOverlay({ query: "shared", rows: [row()], total: 58 });
    expect(screen.getByText("57 more matched — narrow the words to see them")).toBeTruthy();
  });
});

// #479: selecting a result opens it in place — editable if live, read-only if
// history. `ItemPanel` (detail mode) already carries its own exhaustive Edit
// suite; what is proved here is that this overlay reaches it correctly — a
// live result gets an `onTriage`, a Done/archived one does not — and that
// expanding never disturbs the rest of the list.
describe("RecallOverlay — selecting a result (#479)", () => {
  // Offsets from `TEST_NOW`, the same fixed instant `renderOverlay` hands the
  // component as `nowMs`.
  function liveRow(overrides: Partial<RecallRowDTO> = {}): RecallRowDTO {
    return row({
      id: "item-1",
      seq: 42,
      title: "buy stamps",
      description: "the blue ones",
      group: "live",
      createdAt: TEST_NOW - 3 * 24 * 60 * 60 * 1000,
      updatedAt: TEST_NOW - 65 * 60 * 1000,
      ...overrides,
    });
  }

  it("expands a row inline on click: description, HB handle and timestamps, list intact", () => {
    renderOverlay({
      query: "stamps",
      rows: [liveRow(), row({ id: "item-2", title: "second result" })],
      total: 2,
    });

    expect(screen.queryByText("the blue ones")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /buy stamps/i }));

    expect(screen.getByText("the blue ones")).toBeTruthy();
    expect(screen.getByText("HB-42")).toBeTruthy();
    expect(screen.getByText(/created 3d ago/)).toBeTruthy();
    expect(screen.getByText(/updated 1h ago/)).toBeTruthy();
    // Selecting never navigates away — the rest of the result list is still
    // on screen, and the overlay is still the same dialog.
    expect(screen.getByText("second result")).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("a second click on the same row collapses it again", () => {
    renderOverlay({ query: "stamps", rows: [liveRow()], total: 1 });
    const summary = screen.getByRole("button", { name: /buy stamps/i });

    fireEvent.click(summary);
    expect(screen.getByText("the blue ones")).toBeTruthy();

    fireEvent.click(summary);
    expect(screen.queryByText("the blue ones")).toBeNull();
  });

  it("a live result exposes the full edit form, over the shared draft state and the existing mutation path", () => {
    const onTriage = vi.fn();
    renderOverlay({ query: "stamps", rows: [liveRow()], total: 1, onTriage });

    fireEvent.click(screen.getByRole("button", { name: /buy stamps/i }));
    expect(screen.getByRole("button", { name: "Edit" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    expect((screen.getByLabelText("Title") as HTMLInputElement).value).toBe("buy stamps");
  });

  it("editing a live result's title saves through the same triage mutation every other edit uses", () => {
    const onTriage = vi.fn();
    renderOverlay({ query: "stamps", rows: [liveRow()], total: 1, onTriage });

    fireEvent.click(screen.getByRole("button", { name: /buy stamps/i }));
    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByLabelText("Title"), { target: { value: "buy more stamps" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    // `null` destination (#122): editing from Recall leaves the item's stage
    // exactly where it is, the same as every other detail-mode edit.
    expect(onTriage).toHaveBeenCalledWith("item-1", null, { title: "buy more stamps" });
  });

  // #479 round-2 review: `useItemDraft` clears the typed edit the instant
  // `lastTriage` reports `"ok"` for this item, falling back to whatever
  // `row` this render was handed — so an `"ok"` that arrives BEFORE
  // `useRecallWiring` has re-requested `Core::search` reverts the screen to
  // the pre-edit title, which reads exactly like a discarded edit. This is
  // the full round trip a caller behind the fixed wiring actually produces:
  // `lastTriage` and a fresh `rows` answer carrying the saved title arrive
  // together, and what's proved here is that the result *stays* saved once
  // they do — not, and never, that the stale-`rows` window is fine.
  it("an ok triage result closes Edit and the row reflects the saved title, once rows have refreshed", () => {
    const { view } = renderOverlay({ query: "stamps", rows: [liveRow()], total: 1, onTriage: vi.fn() });

    fireEvent.click(screen.getByRole("button", { name: /buy stamps/i }));
    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByLabelText("Title"), { target: { value: "buy more stamps" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    view.rerender(
      <RecallOverlay
        open
        query="stamps"
        onQueryChange={vi.fn()}
        onClose={vi.fn()}
        rows={[liveRow({ title: "buy more stamps" })]}
        total={1}
        projects={[]}
        onTriage={vi.fn()}
        lastTriage={{ kind: "ok", seed: "s1", itemId: "item-1", error: null }}
        nowMs={TEST_NOW}
      />,
    );

    // Edit closed (#222's clear-on-ok) AND the saved title is what's on
    // screen — never the pre-edit "buy stamps" a stale `rows` answer would
    // have left behind. Two elements carry the title (the row's own summary
    // line and `ItemPanel`'s heading), so `getAllByText` rather than
    // `getByText`.
    expect(screen.queryByLabelText("Title")).toBeNull();
    expect(screen.getAllByText("buy more stamps").length).toBeGreaterThan(0);
    expect(screen.queryByText("buy stamps")).toBeNull();
  });

  it("a write failure is stated in the overlay, and the typed edits survive it", () => {
    const { view } = renderOverlay({ query: "stamps", rows: [liveRow()], total: 1, onTriage: vi.fn() });

    fireEvent.click(screen.getByRole("button", { name: /buy stamps/i }));
    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByLabelText("Title"), { target: { value: "buy more stamps" } });

    view.rerender(
      <RecallOverlay
        open
        query="stamps"
        onQueryChange={vi.fn()}
        onClose={vi.fn()}
        rows={[liveRow()]}
        total={1}
        projects={[]}
        onTriage={vi.fn()}
        lastTriage={{ kind: "failed", seed: "s1", itemId: "item-1", error: "could not save" }}
        nowMs={TEST_NOW}
      />,
    );

    expect(screen.getByText("could not save")).toBeTruthy();
    expect((screen.getByLabelText("Title") as HTMLInputElement).value).toBe("buy more stamps");
  });

  it("a Done result renders read-only, with no edit affordance", () => {
    renderOverlay({
      query: "stamps",
      rows: [liveRow({ id: "item-3", group: "done" })],
      total: 1,
      onTriage: vi.fn(),
    });

    fireEvent.click(screen.getByRole("button", { name: /buy stamps/i }));
    expect(screen.getByText("the blue ones")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Edit" })).toBeNull();
  });

  it("an archived result renders read-only, with no edit affordance", () => {
    renderOverlay({
      query: "stamps",
      rows: [liveRow({ id: "item-4", group: "archived" })],
      total: 1,
      onTriage: vi.fn(),
    });

    fireEvent.click(screen.getByRole("button", { name: /buy stamps/i }));
    expect(screen.getByText("the blue ones")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Edit" })).toBeNull();
  });
});
