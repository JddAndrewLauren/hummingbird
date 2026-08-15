// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "../test/component";
import { itemDTO } from "../test/component";
import type { StepDTO } from "../store/protocol";
import { GrillTakeover } from "./GrillTakeover";

const item = itemDTO({ id: "item-1", title: "book flights", stage: "triage" });

function step(overrides: Partial<StepDTO> = {}): StepDTO {
  return {
    id: "step-1",
    itemId: "item-1",
    body: "pack",
    done: false,
    position: 0,
    deletedAt: null,
    version: 1,
    ...overrides,
  };
}

describe("GrillTakeover", () => {
  it("renders the question, its choices and the recommended answer; a choice answers", () => {
    const onAnswer = vi.fn();
    render(
      <GrillTakeover
        item={item}
        steps={[]}
        turn={{
          phase: "question",
          messages: [],
          question: { prompt: "Which airport?", recommendedAnswer: "SEA", choices: ["SEA", "PDX"] },
          backend: null,
          model: null,
        }}
        turns={[]}
        onAnswer={onAnswer}
        onKeepGrilling={() => {}}
        onRetry={() => {}}
        onConfirm={() => {}}
        onBack={() => {}}
        onDiscard={() => {}}
        completionError={null}
      />,
    );

    screen.getByText("Which airport?");
    screen.getByText("Recommended: SEA");
    fireEvent.click(screen.getByText("PDX"));
    expect(onAnswer).toHaveBeenCalledWith("PDX");
  });

  it("free text always answers too, regardless of the listed choices", () => {
    const onAnswer = vi.fn();
    render(
      <GrillTakeover
        item={item}
        steps={[]}
        turn={{
          phase: "question",
          messages: [],
          question: { prompt: "Which airport?", recommendedAnswer: "SEA", choices: ["SEA", "PDX"] },
          backend: null,
          model: null,
        }}
        turns={[]}
        onAnswer={onAnswer}
        onKeepGrilling={() => {}}
        onRetry={() => {}}
        onConfirm={() => {}}
        onBack={() => {}}
        onDiscard={() => {}}
        completionError={null}
      />,
    );

    fireEvent.change(screen.getByLabelText("Or answer in your own words"), {
      target: { value: "Honestly, either works" },
    });
    fireEvent.click(screen.getByText("Answer"));
    expect(onAnswer).toHaveBeenCalledWith("Honestly, either works");
  });

  it("the review card offers Confirm and Keep grilling, and no plan tick when there is no plan to strand", () => {
    render(
      <GrillTakeover
        item={item}
        steps={[]}
        turn={{
          phase: "proposal",
          messages: [],
          proposal: { summary: "Settled on SEA", verdict: "resolved", patch: { title: "book flights to SEA" } },
          backend: null,
          model: null,
        }}
        turns={[]}
        onAnswer={() => {}}
        onKeepGrilling={() => {}}
        onRetry={() => {}}
        onConfirm={() => {}}
        onBack={() => {}}
        onDiscard={() => {}}
        completionError={null}
      />,
    );

    screen.getByText("Confirm");
    screen.getByText("Keep grilling");
    expect(screen.queryByLabelText(/unfinished step/)).toBeNull();
  });

  /** #355's acceptance: confirming a demotion that would strand a live
   * plan requires the explicit, default-off tick naming the step count. */
  it("fog_remains with a live plan offers the tick, default off, naming the step count", () => {
    const onConfirm = vi.fn();
    render(
      <GrillTakeover
        item={item}
        steps={[step(), step({ id: "step-2" })]}
        turn={{
          phase: "proposal",
          messages: [],
          proposal: { summary: "Still foggy", verdict: "fog_remains", patch: {} },
          backend: null,
          model: null,
        }}
        turns={[]}
        onAnswer={() => {}}
        onKeepGrilling={() => {}}
        onRetry={() => {}}
        onConfirm={onConfirm}
        onBack={() => {}}
        onDiscard={() => {}}
        completionError={null}
      />,
    );

    const tick = screen.getByLabelText("Also delete 2 unfinished steps") as HTMLInputElement;
    expect(tick.checked).toBe(false);

    fireEvent.click(screen.getByText("Confirm"));
    expect(onConfirm).toHaveBeenCalledWith(
      expect.objectContaining({ deleteUntickedPlan: false, verdict: "fog_remains" }),
    );

    fireEvent.click(tick);
    fireEvent.click(screen.getByText("Confirm"));
    expect(onConfirm).toHaveBeenLastCalledWith(expect.objectContaining({ deleteUntickedPlan: true }));
  });

  /** BLOCKER 2's fix, from the component's own vantage: `steps === null`
   * ("the session's Steps snapshot has not landed yet") must disable
   * Confirm and offer no plan tick — never read as "no Steps at all". */
  it("disables Confirm and offers no plan tick while the Steps snapshot has not landed", () => {
    const onConfirm = vi.fn();
    render(
      <GrillTakeover
        item={item}
        steps={null}
        turn={{
          phase: "proposal",
          messages: [],
          proposal: { summary: "Still foggy", verdict: "fog_remains", patch: {} },
          backend: null,
          model: null,
        }}
        turns={[]}
        onAnswer={() => {}}
        onKeepGrilling={() => {}}
        onRetry={() => {}}
        onConfirm={onConfirm}
        onBack={() => {}}
        onDiscard={() => {}}
        completionError={null}
      />,
    );

    expect(screen.queryByLabelText(/unfinished step/)).toBeNull();
    expect(screen.getByRole("button", { name: "Confirm" }).hasAttribute("disabled")).toBe(true);
    fireEvent.click(screen.getByText("Confirm"));
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("a declined turn states the reason with alert semantics, and offers Try again", () => {
    const onRetry = vi.fn();
    render(
      <GrillTakeover
        item={item}
        steps={[]}
        turn={{ phase: "declined", messages: [], reason: "The run ended without an answer.", backend: null, model: null, answered: false }}
        turns={[]}
        onAnswer={() => {}}
        onKeepGrilling={() => {}}
        onRetry={onRetry}
        onConfirm={() => {}}
        onBack={() => {}}
        onDiscard={() => {}}
        completionError={null}
      />,
    );

    expect(screen.getByRole("alert").textContent).toBe("The run ended without an answer.");
    fireEvent.click(screen.getByText("Try again"));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it("Back calls onBack", () => {
    const onBack = vi.fn();
    render(
      <GrillTakeover
        item={item}
        steps={[]}
        turn={{ phase: "asking", messages: ["reading item-1"] }}
        turns={[]}
        onAnswer={() => {}}
        onKeepGrilling={() => {}}
        onRetry={() => {}}
        onConfirm={() => {}}
        onBack={onBack}
        onDiscard={() => {}}
        completionError={null}
      />,
    );

    fireEvent.click(screen.getByLabelText("Back to Triage"));
    expect(onBack).toHaveBeenCalledTimes(1);
  });

  /** #356's explicit, confirmed "Discard": a `window.confirm` dialog is
   * the confirmation, and `onDiscard` fires only when the human accepts
   * it — a cancelled dialog must leave the interview standing. */
  it("Discard confirms with the human before calling onDiscard", () => {
    const onDiscard = vi.fn();
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(
      <GrillTakeover
        item={item}
        steps={[]}
        turn={{ phase: "asking", messages: [] }}
        turns={[]}
        onAnswer={() => {}}
        onKeepGrilling={() => {}}
        onRetry={() => {}}
        onConfirm={() => {}}
        onBack={() => {}}
        onDiscard={onDiscard}
        completionError={null}
      />,
    );

    fireEvent.click(screen.getByText("Discard"));
    expect(confirmSpy).toHaveBeenCalledTimes(1);
    expect(onDiscard).toHaveBeenCalledTimes(1);
    confirmSpy.mockRestore();
  });

  it("Discard never calls onDiscard when the human cancels the confirm dialog", () => {
    const onDiscard = vi.fn();
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(
      <GrillTakeover
        item={item}
        steps={[]}
        turn={{ phase: "asking", messages: [] }}
        turns={[]}
        onAnswer={() => {}}
        onKeepGrilling={() => {}}
        onRetry={() => {}}
        onConfirm={() => {}}
        onBack={() => {}}
        onDiscard={onDiscard}
        completionError={null}
      />,
    );

    fireEvent.click(screen.getByText("Discard"));
    expect(onDiscard).not.toHaveBeenCalled();
    confirmSpy.mockRestore();
  });

  /** Non-blocking fix: focus moves into the takeover on mount. */
  it("focus moves into the takeover when it mounts", () => {
    const { container } = render(
      <GrillTakeover
        item={item}
        steps={[]}
        turn={{ phase: "asking", messages: [] }}
        turns={[]}
        onAnswer={() => {}}
        onKeepGrilling={() => {}}
        onRetry={() => {}}
        onConfirm={() => {}}
        onBack={() => {}}
        onDiscard={() => {}}
        completionError={null}
      />,
    );

    expect(document.activeElement).toBe(container.firstChild);
  });

  it("a completion failure states with alert semantics on the review card", () => {
    render(
      <GrillTakeover
        item={item}
        steps={[]}
        turn={{
          phase: "proposal",
          messages: [],
          proposal: { summary: "Settled on SEA", verdict: "resolved", patch: {} },
          backend: null,
          model: null,
        }}
        turns={[]}
        onAnswer={() => {}}
        onKeepGrilling={() => {}}
        onRetry={() => {}}
        onConfirm={() => {}}
        onBack={() => {}}
        onDiscard={() => {}}
        completionError="unticked steps changed since this review was last shown"
      />,
    );

    expect(screen.getByRole("alert").textContent).toBe(
      "unticked steps changed since this review was last shown",
    );
  });

  /** The label must not read as though it edits the item — `applied_patch`
   * is recorded on the Grill and never applied automatically. */
  it("the proposed-edit field says it is recorded, not applied", () => {
    render(
      <GrillTakeover
        item={item}
        steps={[]}
        turn={{
          phase: "proposal",
          messages: [],
          proposal: { summary: "Settled on SEA", verdict: "resolved", patch: {} },
          backend: null,
          model: null,
        }}
        turns={[]}
        onAnswer={() => {}}
        onKeepGrilling={() => {}}
        onRetry={() => {}}
        onConfirm={() => {}}
        onBack={() => {}}
        onDiscard={() => {}}
        completionError={null}
      />,
    );

    screen.getByText(/never applied to the item automatically/i);
  });
});
