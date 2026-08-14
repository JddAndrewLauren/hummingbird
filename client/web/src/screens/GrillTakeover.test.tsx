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
        onConfirm={() => {}}
        onBack={() => {}}
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
        onConfirm={() => {}}
        onBack={() => {}}
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
        onConfirm={() => {}}
        onBack={() => {}}
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
        onConfirm={onConfirm}
        onBack={() => {}}
        completionError={null}
      />,
    );

    const tick = screen.getByLabelText("Also delete 2 unfinished steps") as HTMLInputElement;
    expect(tick.checked).toBe(false);

    fireEvent.click(screen.getByText("Confirm"));
    expect(onConfirm).toHaveBeenCalledWith(
      [step(), step({ id: "step-2" })],
      expect.objectContaining({ deleteUntickedPlan: false, verdict: "fog_remains" }),
    );

    fireEvent.click(tick);
    fireEvent.click(screen.getByText("Confirm"));
    expect(onConfirm).toHaveBeenLastCalledWith(
      expect.anything(),
      expect.objectContaining({ deleteUntickedPlan: true }),
    );
  });

  it("a declined turn states the reason with alert semantics", () => {
    render(
      <GrillTakeover
        item={item}
        steps={[]}
        turn={{ phase: "declined", messages: [], reason: "The run ended without an answer.", backend: null, model: null, answered: false }}
        turns={[]}
        onAnswer={() => {}}
        onKeepGrilling={() => {}}
        onConfirm={() => {}}
        onBack={() => {}}
        completionError={null}
      />,
    );

    expect(screen.getByRole("alert").textContent).toBe("The run ended without an answer.");
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
        onConfirm={() => {}}
        onBack={onBack}
        completionError={null}
      />,
    );

    fireEvent.click(screen.getByLabelText("Back to Triage"));
    expect(onBack).toHaveBeenCalledTimes(1);
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
        onConfirm={() => {}}
        onBack={() => {}}
        completionError="unticked steps changed since this review was last shown"
      />,
    );

    expect(screen.getByRole("alert").textContent).toBe(
      "unticked steps changed since this review was last shown",
    );
  });
});
