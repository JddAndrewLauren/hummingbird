// @vitest-environment jsdom
//
// #358's acceptance, through a mounted history. The anti-goal is checked
// directly: the collapsed list — and the expanded list, until a transcript
// is asked for — never calls the transcript fetch.

import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, grillDTO, render, screen, within } from "../../test/component";
import type { FetchTranscript, TranscriptResult } from "../../grill/transcript-client";
import { GrillHistory, TRANSCRIPT_FAILURE_COPY } from "./GrillHistory";

const NEWER = grillDTO({
  id: "grill-new",
  summary: "Needs two quotes before anything else.",
  verdict: "fog_remains",
  modelProposal: '{"title":"Get a quote"}',
  appliedPatch: '{"title":"Get two quotes"}',
  resultingStage: "grilling",
  completedAt: 9_000,
});
const OLDER = grillDTO({ id: "grill-old", summary: "Done means the tap stops dripping.", completedAt: 1_000 });

function deferred() {
  let resolve!: (result: TranscriptResult) => void;
  const promise = new Promise<TranscriptResult>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

function expandList() {
  fireEvent.click(screen.getByRole("button", { name: /Show 2 grills/ }));
}

describe("GrillHistory", () => {
  it("renders collapsed by default and fetches no transcript", () => {
    const fetchTranscript = vi.fn<FetchTranscript>();
    render(<GrillHistory grills={[NEWER, OLDER]} fetchTranscript={fetchTranscript} nowMs={10_000} />);

    const toggle = screen.getByRole("button", { name: "Show 2 grills" });
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByText(NEWER.summary)).toBeNull();
    expect(fetchTranscript).not.toHaveBeenCalled();
  });

  it("expands newest first, and still fetches no transcript until one is asked for", () => {
    const fetchTranscript = vi.fn<FetchTranscript>();
    render(<GrillHistory grills={[NEWER, OLDER]} fetchTranscript={fetchTranscript} nowMs={10_000} />);

    expandList();

    const entries = within(screen.getByRole("list")).getAllByRole("listitem");
    expect(entries).toHaveLength(2);
    expect(within(entries[0]).getByText(NEWER.summary)).toBeTruthy();
    expect(within(entries[1]).getByText(OLDER.summary)).toBeTruthy();
    expect(within(entries[0]).getByText("Fog remains")).toBeTruthy();
    expect(within(entries[1]).getByText("Resolved")).toBeTruthy();
    expect(fetchTranscript).not.toHaveBeenCalled();
  });

  it("draws the model's proposal and the applied patch as two separately labelled blocks", () => {
    render(<GrillHistory grills={[NEWER]} fetchTranscript={vi.fn()} nowMs={10_000} />);
    fireEvent.click(screen.getByRole("button", { name: "Show 1 grill" }));

    const proposed = screen.getByRole("region", { name: "Proposed by the model" });
    const applied = screen.getByRole("region", { name: "Applied" });
    expect(proposed).not.toBe(applied);
    expect(within(proposed).getByText(/Get a quote/)).toBeTruthy();
    expect(within(proposed).queryByText(/Get two quotes/)).toBeNull();
    expect(within(applied).getByText(/Get two quotes/)).toBeTruthy();
  });

  it("says a patch was applied as proposed rather than drawing it twice", () => {
    render(<GrillHistory grills={[OLDER]} fetchTranscript={vi.fn()} nowMs={10_000} />);
    fireEvent.click(screen.getByRole("button", { name: "Show 1 grill" }));

    expect(screen.getByRole("region", { name: "Proposed by the model" })).toBeTruthy();
    expect(screen.queryByRole("region", { name: "Applied" })).toBeNull();
    expect(screen.getByText("Applied as proposed.")).toBeTruthy();
  });

  it("loads the transcript on expand, with its own pending state, and reads it once", async () => {
    const { promise, resolve } = deferred();
    const fetchTranscript = vi.fn<FetchTranscript>(() => promise);
    render(<GrillHistory grills={[NEWER, OLDER]} fetchTranscript={fetchTranscript} nowMs={10_000} />);
    expandList();

    const [first] = within(screen.getByRole("list")).getAllByRole("listitem");
    fireEvent.click(within(first).getByRole("button", { name: "Show transcript" }));

    expect(fetchTranscript).toHaveBeenCalledTimes(1);
    expect(fetchTranscript).toHaveBeenCalledWith("grill-new");
    expect(within(first).getByRole("status").textContent).toBe("Loading transcript…");

    await act(async () => {
      resolve({ kind: "ok", transcript: "Q: What is the blocker?\nA: A quote." });
    });
    expect(within(first).getByLabelText("Transcript").textContent).toContain("A: A quote.");

    fireEvent.click(within(first).getByRole("button", { name: "Hide transcript" }));
    fireEvent.click(within(first).getByRole("button", { name: "Show transcript" }));
    expect(fetchTranscript).toHaveBeenCalledTimes(1);
  });

  it("shows a failed transcript read as its own state, and retries only on request", async () => {
    const fetchTranscript = vi
      .fn<FetchTranscript>()
      .mockResolvedValueOnce({ kind: "failed", reason: "unreachable" })
      .mockResolvedValueOnce({ kind: "ok", transcript: "Q: Why?\nA: Because." });
    render(<GrillHistory grills={[OLDER]} fetchTranscript={fetchTranscript} nowMs={10_000} />);
    fireEvent.click(screen.getByRole("button", { name: "Show 1 grill" }));

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Show transcript" }));
    });
    expect(screen.getByRole("alert").textContent).toBe(TRANSCRIPT_FAILURE_COPY.unreachable);
    expect(fetchTranscript).toHaveBeenCalledTimes(1);

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    });
    expect(fetchTranscript).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByLabelText("Transcript").textContent).toContain("A: Because.");
  });

  it("renders an honest empty state for an item never grilled — no toggle, no empty box", () => {
    render(<GrillHistory grills={[]} fetchTranscript={vi.fn()} />);

    expect(screen.getByText("Never grilled — no completed grills on this item.")).toBeTruthy();
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.queryByRole("list")).toBeNull();
  });

  it("draws nothing while the mirror read has not answered, rather than claiming never grilled", () => {
    const { container } = render(<GrillHistory grills={undefined} fetchTranscript={vi.fn()} />);
    expect(container.innerHTML).toBe("");
  });
});
