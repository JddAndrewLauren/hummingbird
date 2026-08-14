// @vitest-environment jsdom
//
// #273's acceptance, one test per criterion, through a genuinely mounted
// panel. The pure modules carry their own unit tests; what is proved here
// is the wiring — that each decision reaches the screen, which is the
// failure mode `src/test/component.tsx`'s header exists for.

import { describe, expect, it, vi } from "vitest";
import { fireEvent, itemDTO, render, screen, stepDTO } from "../../test/component";
import { IDLE, reduceRun, type SkillEvent, type SkillRunState } from "../../skills/run-state";
import { ItemDetailPanel } from "./ItemDetailPanel";

function stateFrom(events: SkillEvent[]): SkillRunState {
  return events.reduce(reduceRun, IDLE);
}

const STARTED: SkillEvent = { kind: "started" };

function panel(options: {
  steps?: ReturnType<typeof stepDTO>[];
  run?: SkillRunState;
  onRun?: (request: { itemId: string; replace?: boolean; grain?: number }) => void;
  microtask?: boolean;
  declinedFallback?: { label: string; onSwitchAndRun: (request: { itemId: string }) => void } | null;
} = {}) {
  const onRun = options.onRun ?? vi.fn();
  render(
    <ItemDetailPanel
      item={itemDTO({ id: "item-1", title: "Clean the garage" })}
      steps={options.steps ?? []}
      onClose={() => {}}
      microtask={
        options.microtask === false
          ? undefined
          : { run: options.run ?? IDLE, onRun, declinedFallback: options.declinedFallback }
      }
    />,
  );
  return onRun;
}

describe("the affordance follows the item's own steps", () => {
  it("no steps offers Break into steps, and tapping issues one bare run", () => {
    const onRun = panel();
    const button = screen.getByRole("button", { name: /break into steps/i });
    fireEvent.click(button);
    expect(onRun).toHaveBeenCalledTimes(1);
    expect(onRun).toHaveBeenCalledWith({ itemId: "item-1" });
    expect(screen.queryByLabelText("Grain")).toBeNull();
  });

  /** #307 point 1: ticked steps are record, so an append after them is the
   * normal case. */
  it("all-done steps still offer Break, not Rewrite", () => {
    panel({ steps: [stepDTO({ id: "a", done: true }), stepDTO({ id: "b", done: true })] });
    expect(screen.getByRole("button", { name: /break into steps/i })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /rewrite/i })).toBeNull();
  });

  it("a live undone plan offers Rewrite, counting the undone steps in the label", () => {
    panel({
      steps: [stepDTO({ id: "a", done: true }), stepDTO({ id: "b" }), stepDTO({ id: "c" })],
    });
    expect(screen.getByRole("button", { name: "Rewrite 2 steps" })).toBeTruthy();
  });

  it("one undone step reads in the singular", () => {
    panel({ steps: [stepDTO({ id: "b" })] });
    expect(screen.getByRole("button", { name: "Rewrite 1 step" })).toBeTruthy();
  });

  /** #274 moved which backend/model answers to an app-level preference
   * (Settings) — this panel offers only the grain, never a model of its
   * own. */
  it("a rewrite sends replace and the chosen grain, with no model select on screen", () => {
    const onRun = panel({ steps: [stepDTO({ id: "b" })] });
    expect(screen.queryByLabelText("Model")).toBeNull();
    fireEvent.change(screen.getByLabelText("Grain"), { target: { value: "3" } });
    fireEvent.click(screen.getByRole("button", { name: "Rewrite 1 step" }));
    expect(onRun).toHaveBeenCalledWith({
      itemId: "item-1",
      replace: true,
      grain: 3,
    });
  });

  it("no affordance at all when the panel is given no microtask wiring", () => {
    panel({ microtask: false });
    expect(screen.queryByRole("button", { name: /break into steps/i })).toBeNull();
  });

  /**
   * The whole shape of the feature in one test: the gesture is a function of
   * the steps the normal read path delivered, so when a run's checklist
   * lands at the next sync cycle the affordance flips on its own. Nothing
   * re-decides it, and no run state is consulted.
   */
  it("flips from Break to Rewrite when the run's steps arrive through the read path", () => {
    const onRun = vi.fn();
    const { rerender } = render(
      <ItemDetailPanel
        item={itemDTO({ id: "item-1" })}
        steps={[]}
        onClose={() => {}}
        microtask={{ run: IDLE, onRun }}
      />,
    );
    expect(screen.getByRole("button", { name: /break into steps/i })).toBeTruthy();

    rerender(
      <ItemDetailPanel
        item={itemDTO({ id: "item-1" })}
        steps={[stepDTO({ id: "a" }), stepDTO({ id: "b" }), stepDTO({ id: "c", done: true })]}
        onClose={() => {}}
        microtask={{ run: IDLE, onRun }}
      />,
    );
    expect(screen.queryByRole("button", { name: /break into steps/i })).toBeNull();
    expect(screen.getByRole("button", { name: "Rewrite 2 steps" })).toBeTruthy();
    // And the grain select only exists on the rewrite side.
    expect(screen.getByLabelText("Grain")).toBeTruthy();
  });
});

describe("an in-flight run", () => {
  it("disables the button, so a second tap starts nothing", () => {
    const onRun = panel({ run: stateFrom([STARTED]) });
    const button = screen.getByRole("button", { name: /break into steps/i }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    fireEvent.click(button);
    expect(onRun).not.toHaveBeenCalled();
  });

  it("renders the narration in order, politely", () => {
    panel({
      run: stateFrom([
        STARTED,
        { kind: "progress", message: "reading item-1 from the authority" },
        { kind: "progress", message: "running skill microtask" },
      ]),
    });
    const lines = [...screen.getByRole("status").children].map((node) => node.textContent);
    expect(lines).toEqual(["reading item-1 from the authority", "running skill microtask"]);
  });

  it("a repeated heartbeat renders once, not once per beat", () => {
    panel({
      run: stateFrom([
        STARTED,
        { kind: "progress", message: "still running" },
        { kind: "progress", message: "still running" },
      ]),
    });
    expect(screen.getByRole("status").children).toHaveLength(1);
  });
});

describe("the outcome", () => {
  it("renders the stamp from the envelope, and the note", () => {
    panel({
      run: stateFrom([
        STARTED,
        {
          kind: "ok",
          result: { steps: ["a"], note: "Kept 2 ticked steps." },
          backend: "anthropic",
          model: "claude-opus-5",
        },
      ]),
    });
    expect(screen.getByText("anthropic · claude-opus-5")).toBeTruthy();
    expect(screen.getByText("Kept 2 ticked steps.")).toBeTruthy();
  });

  /** An unstamped envelope means nothing was attempted (ADR-0018) — so
   * nothing is rendered, rather than a name invented here. */
  it("renders no stamp at all when the envelope named no backend", () => {
    panel({
      run: stateFrom([
        STARTED,
        { kind: "failed", error: "Cloud runner unreachable.", backend: null, model: null },
      ]),
    });
    expect(screen.queryByText(/·/)).toBeNull();
    expect(screen.getByRole("alert").textContent).toBe("Cloud runner unreachable.");
  });

  /** #307 made the seam's decline prose-only, with no reason code,
   * precisely so nothing string-matches or reworks it. */
  it("renders the seam's decline byte-identically, unprefixed", () => {
    const reason =
      "This item already has 4 unticked steps; re-run with replace: true to rewrite them.";
    panel({
      run: stateFrom([STARTED, { kind: "failed", error: reason, backend: "anthropic", model: null }]),
    });
    expect(screen.getByRole("alert").textContent).toBe(reason);
  });

  /** Box 7: the stamp "always names the backend and model that actually
   * answered" — a declined answer included, because comparing tiers is the
   * whole point of the picker. Routing used to flatten every non-ok
   * terminal's stamp to `null` on its way here, so this rendered nothing. */
  it("renders the stamp on a decline the backend answered with", () => {
    panel({
      run: stateFrom([
        STARTED,
        {
          kind: "failed",
          error: "That item already has live steps.",
          backend: "anthropic",
          model: "opus",
        },
      ]),
    });
    expect(screen.getByText("anthropic · opus")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toBe("That item already has live steps.");
  });

  it("the button is live again once a run has ended", () => {
    const onRun = panel({
      run: stateFrom([STARTED, { kind: "failed", error: "nope", backend: null, model: null }]),
    });
    const button = screen.getByRole("button", { name: /break into steps/i }) as HTMLButtonElement;
    expect(button.disabled).toBe(false);
    fireEvent.click(button);
    expect(onRun).toHaveBeenCalledTimes(1);
  });

  it("an idle run renders no narration block, no stamp and no decline", () => {
    panel();
    expect(screen.queryByRole("status")).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

/** #274: a pinned, dead backend is never silently rerouted — the picker's
 * one-tap offer is a button beside the decline, not an automatic retry. */
describe("the pinned-backend decline (#274)", () => {
  it("offers a one-tap switch when the caller has a fallback to offer", () => {
    const onSwitchAndRun = vi.fn();
    const onRun = panel({
      run: stateFrom([STARTED, { kind: "failed", error: "Cloud runner is not answering right now.", backend: null, model: null }]),
      declinedFallback: { label: "Home runner", onSwitchAndRun },
    });

    const button = screen.getByRole("button", { name: /switch to home runner/i });
    fireEvent.click(button);

    // One call, carrying the request — switching and re-running are the
    // caller's single operation, because doing them as two here would
    // re-run against the selection this panel was rendered with, i.e. the
    // pin that just declined.
    expect(onSwitchAndRun).toHaveBeenCalledWith({ itemId: "item-1" });
    expect(onRun).not.toHaveBeenCalled();
  });

  it("offers nothing when the caller has no fallback (this slice's single-entry registry)", () => {
    panel({
      run: stateFrom([STARTED, { kind: "failed", error: "Cloud runner is not answering right now.", backend: null, model: null }]),
      declinedFallback: null,
    });
    expect(screen.queryByRole("button", { name: /switch to/i })).toBeNull();
  });

  it("offers nothing while the selection is Auto — nothing to fall back FROM", () => {
    panel({
      run: stateFrom([STARTED, { kind: "failed", error: "nope", backend: null, model: null }]),
    });
    expect(screen.queryByRole("button", { name: /switch to/i })).toBeNull();
  });
});

// #446 verification step 7, as a test rather than a screenshot: unset is a
// legitimate resting state, and a green build plus a capture both miss a
// state-gated call site. This panel is the one surface that draws an
// unjudged dimension rather than omitting it, so it is where the ghost
// variants have to be exercised.
describe("size and energy on the detail panel", () => {
  it("draws both for an item with neither set — ghost glyph, em dash, muted", () => {
    render(
      <ItemDetailPanel
        item={itemDTO({ id: "item-1", title: "Nobody has judged this", size: null, energy: null })}
        steps={[]}
        onClose={() => {}}
      />,
    );
    // `Badge` wraps its children in an inner span, so the text match lands
    // there and the pill carrying the colour is its parent — the same
    // element the glyph sits in, which is the rule: never a colour on the
    // icon without the label.
    const size = screen.getByText(/^size:/).parentElement;
    const energy = screen.getByText(/^energy:/).parentElement;
    expect(size?.textContent).toBe("size:—");
    expect(energy?.textContent).toBe("energy:—");
    // Muted, not escalated: an unmade judgement is not a problem.
    expect(size?.style.color).toBe("var(--text-muted)");
    expect(energy?.style.color).toBe("var(--text-muted)");
    // `not.toBeNull`, not `toBeDefined`: a missing glyph returns `null`,
    // which *is* defined. And the ghost wash is the whole point of this
    // state — every element of both families at the flat unset opacity,
    // never the earned/unearned contrast of a judged level.
    const rings = size?.querySelector("svg");
    const bars = energy?.querySelector("svg");
    expect(rings).not.toBeNull();
    expect(bars).not.toBeNull();
    expect(Array.from(rings!.children, (el) => el.getAttribute("opacity"))).toEqual([
      "0.45",
      "0.45",
      "0.45",
    ]);
    expect(Array.from(bars!.children, (el) => el.getAttribute("opacity"))).toEqual([
      "0.45",
      "0.45",
      "0.45",
    ]);
  });

  it("draws the level and its ramp colour once judged", () => {
    render(
      <ItemDetailPanel
        item={itemDTO({ id: "item-1", title: "A judged item", size: "normal", energy: "high" })}
        steps={[]}
        onClose={() => {}}
      />,
    );
    const size = screen.getByText(/^size:/).parentElement;
    const energy = screen.getByText(/^energy:/).parentElement;
    expect(size?.textContent).toBe("size:NORMAL");
    expect(energy?.textContent).toBe("energy:HIGH");
    expect(size?.style.color).toBe("var(--urgency-soon)");
    expect(energy?.style.color).toBe("var(--urgency-now)");
  });
});
