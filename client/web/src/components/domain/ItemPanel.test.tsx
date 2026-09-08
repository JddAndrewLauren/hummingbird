// @vitest-environment jsdom
//
// #273's acceptance, one test per criterion, through a genuinely mounted
// panel. The pure modules carry their own unit tests; what is proved here
// is the wiring — that each decision reaches the screen, which is the
// failure mode `src/test/component.tsx`'s header exists for.

import { describe, expect, it, vi } from "vitest";
import { cleanup, fileLinkDTO, fireEvent, itemDTO, projectDTO, render, screen, stepDTO } from "../../test/component";
import { IDLE, reduceRun, type SkillEvent, type SkillRunState } from "../../skills/run-state";
import type { TaskItemDTO } from "../../store/protocol";
import { VAULT_PATH_PROBLEM } from "../../screens/triage-form";
import { ItemPanel } from "./ItemPanel";

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
    <ItemPanel
      mode="detail"
      item={itemDTO({ id: "item-1", title: "Clean the garage" })}
      projects={[]}
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
      <ItemPanel
        mode="detail"
        item={itemDTO({ id: "item-1" })}
        projects={[]}
        steps={[]}
        onClose={() => {}}
        microtask={{ run: IDLE, onRun }}
      />,
    );
    expect(screen.getByRole("button", { name: /break into steps/i })).toBeTruthy();

    rerender(
      <ItemPanel
        mode="detail"
        item={itemDTO({ id: "item-1" })}
        projects={[]}
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

// Item detail's Edit mode. A minted action's own fields used to be reachable
// nowhere: `TriageRow`'s editor was the only one in the app, and it is only
// mounted for something still in the inbox. This is the same fields, the same
// draft hook and the same mutation — `Core::triage` with no destination
// (#122), which edits and leaves the stage alone.
describe("ItemPanel — detail mode's Edit", () => {
  function detail(options: { onTriage?: ReturnType<typeof vi.fn>; item?: TaskItemDTO } = {}) {
    const onTriage = options.onTriage ?? vi.fn();
    const view = render(
      <ItemPanel
        mode="detail"
        item={options.item ?? itemDTO({ id: "item-1", title: "Clean the garage" })}
        projects={[]}
        steps={[]}
        onClose={() => {}}
        onTriage={onTriage}
      />,
    );
    return { onTriage, view };
  }

  const edit = () => fireEvent.click(screen.getByRole("button", { name: "Edit" }));

  it("reads as a record until asked, then seeds every field from the item", () => {
    detail({
      item: itemDTO({
        id: "item-1",
        title: "Clean the garage",
        description: "the far bay",
        context: "@home",
        deadline: "2026-09-01T09:30",
      }),
    });

    expect(screen.queryByLabelText("Title")).toBeNull();

    edit();
    expect((screen.getByLabelText("Title") as HTMLInputElement).value).toBe("Clean the garage");
    expect((screen.getByLabelText("Description") as HTMLTextAreaElement).value).toBe("the far bay");
    expect((screen.getByLabelText("Context") as HTMLInputElement).value).toBe("@home");
    // Split across the deadline field's two controls, as its own tests pin.
    expect((screen.getByLabelText("Deadline") as HTMLInputElement).value).toBe("2026-09-01");
    expect((screen.getByLabelText("Time") as HTMLInputElement).value).toBe("09:30");
  });

  it("saves only what changed, and leaves the stage alone", () => {
    const { onTriage } = detail();
    edit();
    fireEvent.change(screen.getByLabelText("Description"), { target: { value: "the far bay" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    // `null` destination: an item in detail is already past triage, and
    // there is no destination for "where it already was".
    expect(onTriage).toHaveBeenCalledWith("item-1", null, { description: "the far bay" });
  });

  it("disables Save until something actually differs", () => {
    detail();
    edit();
    const save = () => screen.getByRole("button", { name: "Save" });
    expect(save().hasAttribute("disabled")).toBe(true);

    fireEvent.change(screen.getByLabelText("Description"), { target: { value: "x" } });
    expect(save().hasAttribute("disabled")).toBe(false);
  });

  it("keeps the typing and the open editor when the write comes back failed", () => {
    // #222, one surface over: a failed save must not take the reader's work
    // with it, and must not close the editor they would retry from.
    const { view } = detail();
    edit();
    fireEvent.change(screen.getByLabelText("Description"), { target: { value: "the far bay" } });

    view.rerender(
      <ItemPanel
        mode="detail"
        item={itemDTO({ id: "item-1", title: "Clean the garage" })}
        projects={[]}
        steps={[]}
        onClose={() => {}}
        onTriage={vi.fn()}
        lastTriage={{ kind: "failed", seed: "s1", itemId: "item-1", error: "boom" }}
      />,
    );

    expect((screen.getByLabelText("Description") as HTMLTextAreaElement).value).toBe("the far bay");
    // And the failure is on screen rather than swallowed.
    expect(screen.getByText("boom")).toBeTruthy();
  });

  it("leaves Edit only once the write lands ok", () => {
    const { view } = detail();
    edit();
    fireEvent.change(screen.getByLabelText("Description"), { target: { value: "the far bay" } });

    view.rerender(
      <ItemPanel
        mode="detail"
        item={itemDTO({ id: "item-1", title: "Clean the garage" })}
        projects={[]}
        steps={[]}
        onClose={() => {}}
        onTriage={vi.fn()}
        lastTriage={{ kind: "ok", seed: "s1", itemId: "item-1", error: null }}
      />,
    );

    expect(screen.queryByLabelText("Description")).toBeNull();
    expect(screen.getByRole("button", { name: "Edit" })).toBeTruthy();
  });

  it("discards back to the item's own values without sending anything", () => {
    const { onTriage } = detail();
    edit();
    fireEvent.change(screen.getByLabelText("Description"), { target: { value: "the far bay" } });
    fireEvent.click(screen.getByRole("button", { name: "Discard" }));

    expect(onTriage).not.toHaveBeenCalled();
    expect(screen.queryByLabelText("Description")).toBeNull();
    edit();
    expect((screen.getByLabelText("Description") as HTMLTextAreaElement).value).toBe("");
  });

  it("offers no Edit at all without an onTriage", () => {
    render(
      <ItemPanel
        mode="detail"
        item={itemDTO({ id: "item-1" })}
        projects={[]}
        steps={[]}
        onClose={() => {}}
      />,
    );
    expect(screen.queryByRole("button", { name: "Edit" })).toBeNull();
  });
});

// #479 round-2 review: a caller with no steps wiring of its own (Recall's
// expanded result) must not get "No Steps yet." — a claim this panel has no
// way to know is true when nobody ever asked `Core` for this item's steps.
describe("ItemPanel — showSteps (#479)", () => {
  it("renders the steps block by default, same as every existing caller", () => {
    render(
      <ItemPanel
        mode="detail"
        item={itemDTO({ id: "item-1" })}
        projects={[]}
        steps={[]}
        onClose={() => {}}
      />,
    );
    expect(screen.getByText("No Steps yet.")).toBeTruthy();
  });

  it("renders no steps block at all with showSteps={false}, whatever steps holds", () => {
    render(
      <ItemPanel
        mode="detail"
        item={itemDTO({ id: "item-1" })}
        projects={[]}
        onClose={() => {}}
        showSteps={false}
      />,
    );
    expect(screen.queryByText("No Steps yet.")).toBeNull();
    expect(screen.queryByText("steps")).toBeNull();
  });
});

// #359: Grill reaches Now, and Now's own "row" is a selected card that opens
// into this component's `"detail"` mode — so the Grill button `TriageRow`
// already threads into `"triage"` mode has to reach here too, gated by the
// same `item-actions.ts`'s `canGrill`.
describe("ItemPanel — detail mode's Grill me (#359)", () => {
  it("offers Grill me for a Ready item", () => {
    render(
      <ItemPanel
        mode="detail"
        item={itemDTO({ id: "item-1", stage: "ready" })}
        projects={[]}
        steps={[]}
        onClose={() => {}}
        onGrillMe={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: /grill me/i })).toBeDefined();
  });

  it("calls onGrillMe with the item id", () => {
    const onGrillMe = vi.fn();
    render(
      <ItemPanel
        mode="detail"
        item={itemDTO({ id: "item-1", stage: "in_progress" })}
        projects={[]}
        steps={[]}
        onClose={() => {}}
        onGrillMe={onGrillMe}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /grill me/i }));
    expect(onGrillMe).toHaveBeenCalledWith("item-1");
  });

  it("reads Resume grill once the item carries a draft", () => {
    render(
      <ItemPanel
        mode="detail"
        item={itemDTO({ id: "item-1", stage: "ready" })}
        projects={[]}
        steps={[]}
        onClose={() => {}}
        onGrillMe={vi.fn()}
        hasGrillDraft
      />,
    );
    expect(screen.getByRole("button", { name: "Resume grill" })).toBeDefined();
  });

  it("offers no Grill me for a Blocked or Done item — not reachable from either frontier row", () => {
    for (const stage of ["blocked", "done"] as const) {
      const { unmount } = render(
        <ItemPanel
          mode="detail"
          item={itemDTO({ id: "item-1", stage })}
          projects={[]}
          steps={[]}
          onClose={() => {}}
          onGrillMe={vi.fn()}
        />,
      );
      expect(screen.queryByRole("button", { name: /grill me/i })).toBeNull();
      unmount();
    }
  });

  it("offers no Grill me at all without an onGrillMe", () => {
    render(
      <ItemPanel
        mode="detail"
        item={itemDTO({ id: "item-1", stage: "ready" })}
        projects={[]}
        steps={[]}
        onClose={() => {}}
      />,
    );
    expect(screen.queryByRole("button", { name: /grill me/i })).toBeNull();
  });
});

// #446 verification step 7, as a test rather than a screenshot: unset is a
// legitimate resting state, and a green build plus a capture both miss a
// state-gated call site. This panel is the one surface that draws an
// unjudged dimension rather than omitting it, so it is where the ghost
// variants have to be exercised.
//
// Written against `ItemDetailPanel` on main; retargeted at `ItemPanel`'s
// detail mode, which absorbed that component on this branch.
describe("size and energy on the detail panel", () => {
  it("draws both for an item with neither set — ghost glyph, em dash, muted", () => {
    render(
      <ItemPanel
        mode="detail"
        item={itemDTO({ id: "item-1", title: "Nobody has judged this", size: null, energy: null })}
        projects={[]}
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
      <ItemPanel
        mode="detail"
        item={itemDTO({ id: "item-1", title: "A judged item", size: "normal", energy: "high" })}
        projects={[]}
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

// #631, ADR-0030 decision 3, entry point 1: the copy is decided in
// `triage-form.ts` but the *promotion* half of the gate is knowledge only
// this panel has — the stage change leaves no trace in the field diff — so
// what is proved here is that the promote button actually passes it.
describe("ItemPanel — promotion's copy-at-mint wiring", () => {
  const project = projectDTO({ id: "p1", name: "House repairs", defaultContext: "@computer" });

  function triage(item: TaskItemDTO) {
    const onTriage = vi.fn();
    render(
      <ItemPanel
        mode="triage"
        item={item}
        projects={[project]}
        steps={[]}
        onTriage={onTriage}
      />,
    );
    return onTriage;
  }

  it("copies the project's default onto a capture that arrived already tagged with it", () => {
    const onTriage = triage(itemDTO({ id: "item-9", projectId: "p1", context: null }));

    fireEvent.click(screen.getByRole("button", { name: "Promote to ready" }));

    expect(onTriage).toHaveBeenCalledWith("item-9", "ready", { context: "@computer" });
  });

  it("leaves a context the item already carries alone", () => {
    const onTriage = triage(itemDTO({ id: "item-9", projectId: "p1", context: "@errands" }));

    fireEvent.click(screen.getByRole("button", { name: "Promote to ready" }));

    expect(onTriage).toHaveBeenCalledWith("item-9", "ready", {});
  });
});

describe("the file links block (ADR-0036)", () => {
  function detail(options: {
    fileLinks?: ReturnType<typeof fileLinkDTO>[];
    withWiring?: boolean;
    localRoot?: string | null;
    showFileLinks?: boolean;
    lastWrite?: { seed: string; itemId: string; kind: "ok" | "failed"; error: string | null } | null;
  } = {}) {
    const createFileLink = vi.fn(() => "seed-create");
    const removeFileLink = vi.fn(() => "seed-remove");
    const panel = (lastWrite: typeof options.lastWrite) => (
      <ItemPanel
        mode="detail"
        item={itemDTO({ id: "item-1", title: "Fit the tap washer" })}
        projects={[]}
        steps={[]}
        fileLinks={options.fileLinks ?? []}
        showFileLinks={options.showFileLinks}
        fileLinksWiring={
          options.withWiring === false
            ? undefined
            : { localRoot: options.localRoot ?? null, createFileLink, removeFileLink }
        }
        lastFileLinkWrite={lastWrite ?? null}
      />
    );
    const { rerender } = render(panel(options.lastWrite));
    return {
      createFileLink,
      removeFileLink,
      rerender: (lastWrite: typeof options.lastWrite) => rerender(panel(lastWrite)),
    };
  }

  /** Adding a file link is `FileAttach`, in the attachment row above the
   * list — one click to reveal the field, which is the same gesture the
   * link and note affordances beside it use. */
  function openFileAttach() {
    fireEvent.click(screen.getByRole("button", { name: "Add a file" }));
  }

  it("draws nothing at all with no wiring and no links", () => {
    detail({ withWiring: false });
    expect(screen.queryByText("files")).toBeNull();
    expect(screen.queryByRole("button", { name: "Add a file" })).toBeNull();
  });

  it("draws nothing when the caller never asked for file links", () => {
    detail({ showFileLinks: false });
    expect(screen.queryByText("files")).toBeNull();
  });

  /** …including the Add control, which lives in the attachment row above and
   * so is gated separately. Wiring alone is not enough: a caller that asked
   * for no file links draws neither the resulting list nor the failure line,
   * so an Add here would be a write whose success AND whose failure are both
   * invisible. */
  it("offers no Add a file when the caller never asked for file links, wiring or not", () => {
    detail({ showFileLinks: false });
    expect(screen.queryByRole("button", { name: "Add a file" })).toBeNull();
  });

  it("Open fires the helper's scheme, and the dropbox.com fallback is always drawn", () => {
    const open = vi.spyOn(window, "open").mockImplementation(() => null);
    detail({ fileLinks: [fileLinkDTO({ path: "Finance/2026/receipt.pdf" })] });

    expect(screen.getByText("receipt.pdf")).toBeTruthy();
    expect(screen.getByText("Finance/2026")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    expect(open).toHaveBeenCalledWith(
      "hummingbird-open:?path=Finance%2F2026%2Freceipt.pdf",
      "_blank",
      "noopener,noreferrer",
    );
    expect((screen.getByRole("link", { name: "on dropbox.com" }) as HTMLAnchorElement).href).toBe(
      "https://www.dropbox.com/home/Finance/2026?preview=receipt.pdf",
    );
    open.mockRestore();
  });

  it("a stored path that climbs out gets Remove but no Open", () => {
    detail({ fileLinks: [fileLinkDTO({ path: "../secrets.txt" })] });
    expect(screen.queryByRole("button", { name: "Open" })).toBeNull();
    expect(screen.queryByRole("link", { name: "on dropbox.com" })).toBeNull();
    expect(screen.getByRole("button", { name: "Remove file link" })).toBeTruthy();
  });

  it("Add normalizes a pasted Windows path against this device's root", () => {
    const { createFileLink } = detail({ localRoot: "C:\\Dropbox" });
    openFileAttach();
    fireEvent.change(screen.getByLabelText("File path"), {
      target: { value: '"C:\\Dropbox\\Finance\\2026\\receipt.pdf"' },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(createFileLink).toHaveBeenCalledWith("item-1", "Finance/2026/receipt.pdf");
    // The editor closes on a sent write, leaving the offer to add another.
    expect(screen.queryByLabelText("File path")).toBeNull();
    expect(screen.getByRole("button", { name: "Add a file" })).toBeTruthy();
  });

  it("Add refuses a path that never became relative, and writes nothing", () => {
    const { createFileLink } = detail({ localRoot: null });
    openFileAttach();
    fireEvent.change(screen.getByLabelText("File path"), { target: { value: "C:\\Elsewhere\\x.pdf" } });
    expect(screen.getByText(/relative to Dropbox/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(createFileLink).not.toHaveBeenCalled();
  });

  it("Remove hands the row to the wiring", () => {
    const link = fileLinkDTO({ id: "fl-9", path: "House/Plumbing" });
    const { removeFileLink } = detail({ fileLinks: [link] });
    fireEvent.click(screen.getByRole("button", { name: "Remove file link" }));
    expect(removeFileLink).toHaveBeenCalledWith(link);
  });

  it("Remove is one write at a time: the trash and Add sit disabled until this panel's own ok lands", () => {
    const link = fileLinkDTO({ id: "fl-9", path: "House/Plumbing" });
    const { removeFileLink, rerender } = detail({ fileLinks: [link] });
    const trash = () => screen.getByRole("button", { name: "Remove file link" }) as HTMLButtonElement;
    fireEvent.click(trash());
    fireEvent.click(trash());
    expect(removeFileLink).toHaveBeenCalledTimes(1);
    expect(trash().disabled).toBe(true);
    openFileAttach();
    expect(
      (screen.getByRole("button", { name: "Add" }) as HTMLButtonElement).disabled,
    ).toBe(true);

    // Some other panel's result changes nothing here.
    rerender({ seed: "someone-else", itemId: "item-2", kind: "ok", error: null });
    expect(trash().disabled).toBe(true);

    rerender({ seed: "seed-remove", itemId: "item-1", kind: "ok", error: null });
    expect(trash().disabled).toBe(false);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("a stored back-slashed path is read with / separators everywhere", () => {
    detail({ fileLinks: [fileLinkDTO({ path: "Finance\\2026\\receipt.pdf" })] });
    expect(screen.getByText("receipt.pdf")).toBeTruthy();
    expect(screen.getByText("Finance/2026")).toBeTruthy();
    expect((screen.getByRole("link", { name: "on dropbox.com" }) as HTMLAnchorElement).href).toBe(
      "https://www.dropbox.com/home/Finance/2026?preview=receipt.pdf",
    );
  });

  it("a back-slashed traversal gets Remove but no Open", () => {
    detail({ fileLinks: [fileLinkDTO({ path: "a\\..\\b.pdf" })] });
    expect(screen.queryByRole("button", { name: "Open" })).toBeNull();
    expect(screen.getByRole("button", { name: "Remove file link" })).toBeTruthy();
  });

  it("without wiring the list is read-only: Open and the fallback, no Add, no Remove", () => {
    detail({ withWiring: false, fileLinks: [fileLinkDTO({ path: "House/Plumbing" })] });
    expect(screen.getByRole("button", { name: "Open" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Add a file" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Remove file link" })).toBeNull();
  });

  it("names a failed write only once it is this panel's own", () => {
    const { createFileLink } = detail({ lastWrite: { seed: "seed-create", itemId: "item-1", kind: "failed", error: "nope" } });
    expect(screen.queryByRole("alert")).toBeNull();
    openFileAttach();
    fireEvent.change(screen.getByLabelText("File path"), { target: { value: "x.pdf" } });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(createFileLink).toHaveBeenCalled();
    expect(screen.getByRole("alert").textContent).toBe("nope");
  });
});

/** #771's note affordance, as redrawn by the link/open/edit-or-unlink pass.
 * Every branch turns on two facts — whether a vault is bound, and whether the
 * item already points at a note — plus, for the writing gestures, whether
 * there is an `onTriage` to record them. */
describe("the Obsidian note affordance", () => {
  function detail(options: {
    vaultName?: string | null;
    vaultPath?: string | null;
    title?: string;
    pending?: boolean;
    onTriage?: ReturnType<typeof vi.fn>;
    withTriage?: boolean;
  }) {
    const onTriage = options.onTriage ?? vi.fn();
    render(
      <ItemPanel
        mode="detail"
        item={itemDTO({
          id: "item-1",
          title: options.title ?? "Knee rehab",
          vaultPath: options.vaultPath ?? null,
          pending: options.pending ?? false,
        })}
        projects={[]}
        steps={[]}
        vaultName={options.vaultName ?? null}
        onTriage={options.withTriage === false ? undefined : onTriage}
      />,
    );
    return onTriage;
  }

  /** Any control the note affordance draws — `Add a note`, `Open note`, or
   * the `…` beside it. Deliberately not `/link/i` any more: the link
   * affordance now sits in the same row and would match it. */
  function noteButton() {
    return screen.queryByRole("button", { name: /note/i });
  }

  /** The editor's own field. It carries no label — the button that opened it
   * is the label — so it is found by the placeholder the Edit form uses. */
  function pathField(): HTMLInputElement {
    return screen.getByPlaceholderText("Hummingbird/Knee rehab.md") as HTMLInputElement;
  }

  it("draws nothing at all when no vault is bound", () => {
    detail({ vaultName: null, vaultPath: "Hummingbird/Knee rehab.md" });
    expect(noteButton()).toBeNull();
  });

  it("prefills the editor with the derived path for an item pointing at nothing, and writes nothing until it is confirmed", () => {
    const open = vi.spyOn(window, "open").mockImplementation(() => null);
    const onTriage = detail({ vaultName: "JDD" });

    fireEvent.click(screen.getByRole("button", { name: "Add a note" }));

    expect(pathField().value).toBe("Hummingbird/Knee rehab.md");
    expect(onTriage).not.toHaveBeenCalled();
    expect(open).not.toHaveBeenCalled();
    open.mockRestore();
  });

  /** The proposal is only ever a proposal: what gets stored is whatever
   * stands in the field when Link is pressed. */
  it("stores the edited path, and does not open the note off the back of it", () => {
    const open = vi.spyOn(window, "open").mockImplementation(() => null);
    const onTriage = detail({ vaultName: "JDD" });

    fireEvent.click(screen.getByRole("button", { name: "Add a note" }));
    fireEvent.change(pathField(), { target: { value: "Reading/Knee.md" } });
    fireEvent.click(screen.getByRole("button", { name: "Link" }));

    expect(onTriage).toHaveBeenCalledWith("item-1", null, { vaultPath: "Reading/Knee.md" });
    expect(open).not.toHaveBeenCalled();
    open.mockRestore();
  });

  it("offers Open note for an item that already points at one, and writes nothing", () => {
    const open = vi.spyOn(window, "open").mockImplementation(() => null);
    const onTriage = detail({ vaultName: "JDD", vaultPath: "Reading/Knee.md" });

    fireEvent.click(screen.getByRole("button", { name: "Open note" }));

    expect(onTriage).not.toHaveBeenCalled();
    expect(open).toHaveBeenCalledWith(
      "obsidian://new?vault=JDD&file=Reading%2FKnee.md&append",
      "_blank",
      "noopener,noreferrer",
    );
    open.mockRestore();
  });

  it("reopens the editor over the STORED path, not the derived one, and saves the change", () => {
    const onTriage = detail({ vaultName: "JDD", vaultPath: "Reading/Knee.md" });

    fireEvent.click(screen.getByRole("button", { name: "Edit or remove the note link" }));
    expect(pathField().value).toBe("Reading/Knee.md");

    fireEvent.change(pathField(), { target: { value: "Reading/Knee rehab.md" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(onTriage).toHaveBeenCalledWith("item-1", null, {
      vaultPath: "Reading/Knee rehab.md",
    });
  });

  /** Unlinking is a `null`, exactly as emptying the Edit form's field is —
   * the note itself is never touched. */
  it("clears the pointer through Remove link", () => {
    const onTriage = detail({ vaultName: "JDD", vaultPath: "Reading/Knee.md" });

    fireEvent.click(screen.getByRole("button", { name: "Edit or remove the note link" }));
    fireEvent.click(screen.getByRole("button", { name: "Remove link" }));

    expect(onTriage).toHaveBeenCalledWith("item-1", null, { vaultPath: null });
  });

  /** The shape rule is `vault-uri.ts`'s and the wording is `triage-form.ts`'s
   * — this editor is a second door onto one column, not a second opinion
   * about it. */
  it("refuses to send a path that leaves the vault, and says so in the form's own words", () => {
    const onTriage = detail({ vaultName: "JDD" });

    fireEvent.click(screen.getByRole("button", { name: "Add a note" }));
    fireEvent.change(pathField(), { target: { value: "../outside.md" } });

    expect(screen.getByText(VAULT_PATH_PROBLEM)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Link" }).hasAttribute("disabled")).toBe(true);
    expect(onTriage).not.toHaveBeenCalled();
  });

  it("blocks the save while a mutation on the item is unconfirmed", () => {
    detail({ vaultName: "JDD", vaultPath: "Reading/Knee.md", pending: true });

    fireEvent.click(screen.getByRole("button", { name: "Edit or remove the note link" }));

    expect(screen.getByRole("button", { name: "Save" }).hasAttribute("disabled")).toBe(true);
    expect(screen.getByRole("button", { name: "Remove link" }).hasAttribute("disabled")).toBe(
      true,
    );
  });

  /** A panel with no worker behind it (demo mode) cannot persist a path, so
   * it never offers to — a button that silently
   * records nothing is worse than no button. */
  it("offers no Add a note without an onTriage to record the path", () => {
    detail({ vaultName: "JDD", withTriage: false });
    expect(noteButton()).toBeNull();
  });

  /** …but reopening a note the item already records writes nothing, so that
   * one is still offered — without the edit affordance beside it, which
   * would have nowhere to send what it collected. */
  it("still offers Open note without an onTriage, and nothing to edit it with", () => {
    detail({ vaultName: "JDD", vaultPath: "Reading/Knee.md", withTriage: false });
    expect(screen.getByRole("button", { name: "Open note" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Edit or remove the note link" })).toBeNull();
  });

  /** The stored path gets the same shape rule as a typed one. `vault_path`
   * is a plain column the authority only checks for non-blankness, so a
   * writer that is not this form — `sweep.py`, a skill, the agent — can put
   * a path there that this client refuses to send. It refuses by drawing no
   * affordance at all, which leaves the Edit form as the one place such a
   * path can be repaired. */
  it("offers no Open note for a stored absolute path", () => {
    detail({ vaultName: "JDD", vaultPath: "/Users/john/secrets.md" });
    expect(noteButton()).toBeNull();
  });

  it("offers no Open note for a stored path that climbs out of the vault", () => {
    detail({ vaultName: "JDD", vaultPath: "Hummingbird/../../outside.md" });
    expect(noteButton()).toBeNull();
  });

  /** A title of nothing but stripped characters derives no name at all, and
   * `Hummingbird/.md` is a hidden note every such item would share — so
   * there is nothing to propose and nothing to link. */
  it("offers no Add a note for a title that strips to an empty name", () => {
    detail({ vaultName: "JDD", title: "???" });
    expect(noteButton()).toBeNull();
  });

  it("edits the path through the ordinary triage save, and an emptied field clears it", () => {
    const onTriage = vi.fn();
    render(
      <ItemPanel
        mode="triage"
        item={itemDTO({ id: "item-2", vaultPath: "Hummingbird/Old.md" })}
        projects={[]}
        steps={[]}
        onTriage={onTriage}
      />,
    );

    fireEvent.change(screen.getByLabelText("Vault path"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "Promote to ready" }));

    expect(onTriage).toHaveBeenCalledWith("item-2", "ready", { vaultPath: null });
  });
});

/** #782's Link, as `LinkAttach` draws it — the same three states `NoteLink`
 * has, in the same row. It replaced the anchor-plus-`Edit link` pair: the
 * button carries the same `linkDisplayLabel` text and the same target, and
 * the `…` beside it opens an editor of its own rather than the whole Edit
 * form. */
describe("the Link affordance", () => {
  function detail(options: {
    linkUrl?: string | null;
    linkLabel?: string | null;
    withTriage?: boolean;
  }) {
    const onTriage = vi.fn();
    render(
      <ItemPanel
        mode="detail"
        item={itemDTO({
          id: "item-1",
          linkUrl: options.linkUrl ?? null,
          linkLabel: options.linkLabel ?? null,
        })}
        projects={[]}
        steps={[]}
        onTriage={options.withTriage === false ? undefined : onTriage}
      />,
    );
    return onTriage;
  }

  /** A real anchor, not a button calling `window.open`: this control's whole
   * job is to go somewhere, and the role is what gives it middle-click,
   * modifier-click, "Copy link address" and a place in a screen reader's
   * link list. The panel drew an `<a>` before this row existed, and the move
   * into the row must not have cost it. */
  it("is an anchor named by the label, opening in a new tab", () => {
    const onTriage = detail({ linkUrl: "https://www.youtube.com/watch?v=abc", linkLabel: "Rehab" });

    const anchor = screen.getByRole("link", { name: "Rehab" });

    expect(anchor.getAttribute("href")).toBe("https://www.youtube.com/watch?v=abc");
    expect(anchor.getAttribute("target")).toBe("_blank");
    expect(anchor.getAttribute("rel")).toBe("noopener noreferrer");
    expect(onTriage).not.toHaveBeenCalled();
  });

  it("names an unnamed link by its host", () => {
    detail({ linkUrl: "https://www.youtube.com/watch?v=abc" });
    expect(screen.getByRole("link", { name: "youtube.com" })).toBeTruthy();
  });

  it("offers Add for an item with no link at all", () => {
    detail({});
    expect(screen.getByRole("button", { name: "Add a link" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Edit or remove the link" })).toBeNull();
  });

  /** The column is plain text the authority only checks for non-blankness,
   * so a writer that is not this client can leave a scheme this one would
   * not vouch for. It draws no way to follow such a URL — but it still draws
   * the `…`, because the Edit form is no longer the door to repairing it. */
  it("refuses to follow a non-http link, but still offers to repair it", () => {
    detail({ linkUrl: "javascript:alert(1)", linkLabel: "Nope" });
    expect(screen.queryByRole("link", { name: "Nope" })).toBeNull();
    expect(screen.getByRole("button", { name: "Edit or remove the link" })).toBeTruthy();
  });

  it("the … opens an editor over the stored pair, and saves both halves", () => {
    const onTriage = detail({ linkUrl: "https://example.test/x", linkLabel: "Old" });

    fireEvent.click(screen.getByRole("button", { name: "Edit or remove the link" }));
    expect((screen.getByLabelText("URL") as HTMLInputElement).value).toBe("https://example.test/x");
    expect((screen.getByLabelText("Link name") as HTMLInputElement).value).toBe("Old");

    fireEvent.change(screen.getByLabelText("Link name"), { target: { value: "New" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(onTriage).toHaveBeenCalledWith("item-1", null, {
      linkUrl: "https://example.test/x",
      linkLabel: "New",
    });
  });

  /** Emptying the URL takes the name with it in the form, so what is left
   * can never read as "a name beside no URL" — the one state the pair may
   * not be in, and the one `linkLabelProblem` reports. */
  it("emptying the URL clears the name, and a name alone blocks the save", () => {
    detail({ linkUrl: "https://example.test/x", linkLabel: "Old" });

    fireEvent.click(screen.getByRole("button", { name: "Edit or remove the link" }));
    fireEvent.change(screen.getByLabelText("URL"), { target: { value: "" } });
    expect((screen.getByLabelText("Link name") as HTMLInputElement).value).toBe("");

    fireEvent.change(screen.getByLabelText("Link name"), { target: { value: "Stranded" } });
    expect(screen.getByRole("button", { name: "Save" }).hasAttribute("disabled")).toBe(true);
  });

  it("Remove link clears both halves", () => {
    const onTriage = detail({ linkUrl: "https://example.test/x", linkLabel: "Old" });

    fireEvent.click(screen.getByRole("button", { name: "Edit or remove the link" }));
    fireEvent.click(screen.getByRole("button", { name: "Remove link" }));

    expect(onTriage).toHaveBeenCalledWith("item-1", null, { linkUrl: null, linkLabel: null });
  });

  /** Read-only AND unfollowable leaves nothing worth drawing — an `…` with
   * nowhere to send what it collects is not an affordance. */
  it("draws nothing for an unfollowable link with no onTriage", () => {
    detail({ linkUrl: "mailto:someone@example.test", withTriage: false });
    expect(screen.queryByRole("button", { name: "Edit or remove the link" })).toBeNull();
    expect(screen.queryByRole("link")).toBeNull();
  });

  /** A panel with no worker behind it (demo mode) can still follow a link —
   * that writes nothing — but never offers to change one. */
  it("without an onTriage it follows but never edits, and offers no Add", () => {
    detail({ linkUrl: "https://example.test/x", withTriage: false });
    expect(screen.getByRole("link", { name: "example.test" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Edit or remove the link" })).toBeNull();
    cleanup();
    detail({ withTriage: false });
    expect(screen.queryByRole("button", { name: "Add a link" })).toBeNull();
  });

  it("clearing the URL through the triage save sends both halves as null", () => {
    const onTriage = vi.fn();
    render(
      <ItemPanel
        mode="triage"
        item={itemDTO({ id: "item-2", linkUrl: "https://example.test/x", linkLabel: "Ex" })}
        projects={[]}
        steps={[]}
        onTriage={onTriage}
      />,
    );
    fireEvent.change(screen.getByLabelText("URL"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "Promote to ready" }));
    expect(onTriage).toHaveBeenCalledWith("item-2", "ready", { linkUrl: null, linkLabel: null });
  });

  it("refuses a name typed beside no URL", () => {
    const onTriage = vi.fn();
    render(
      <ItemPanel mode="triage" item={itemDTO({ id: "item-3" })} projects={[]} steps={[]} onTriage={onTriage} />,
    );
    fireEvent.change(screen.getByLabelText("Link name"), { target: { value: "Ex" } });
    fireEvent.click(screen.getByRole("button", { name: "Promote to ready" }));
    expect(onTriage).not.toHaveBeenCalled();
    expect(screen.getByText("A link name needs a URL")).toBeTruthy();
  });
});
