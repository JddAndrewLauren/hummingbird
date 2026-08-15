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
import { render, screen } from "../test/component";

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
    archivedAt: null,
    createdAt: 1_000,
    updatedAt: 1_000,
    version: 1,
    pending: false,
    group: "live",
    ...overrides,
  };
}

function renderOverlay(
  options: {
    query?: string;
    rows?: RecallRowDTO[] | null;
    total?: number;
  } = {},
) {
  const onQueryChange = vi.fn();
  const onClose = vi.fn();
  render(
    <RecallOverlay
      open
      query={options.query ?? ""}
      onQueryChange={onQueryChange}
      onClose={onClose}
      rows={options.rows ?? null}
      total={options.total ?? 0}
    />,
  );
  return { onQueryChange, onClose };
}

describe("RecallOverlay", () => {
  it("renders nothing over the closed prop", () => {
    render(
      <RecallOverlay open={false} query="" onQueryChange={vi.fn()} onClose={vi.fn()} rows={null} total={0} />,
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
