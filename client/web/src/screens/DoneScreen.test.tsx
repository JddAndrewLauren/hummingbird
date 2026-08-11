// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { itemDTO, render, screen, taskState } from "../test/component";
import { DoneScreen } from "./DoneScreen";

const MINUTE = 60_000;

describe("DoneScreen", () => {
  it("renders 'not read yet' while done is null — 'nothing completed' is a claim", () => {
    render(<DoneScreen task={taskState()} nowMs={10 * MINUTE} />);

    expect(screen.getByText("not read yet")).toBeTruthy();
    expect(screen.queryByText("Nothing completed yet")).toBeNull();
  });

  it("renders the empty state only for a real, empty answer", () => {
    render(<DoneScreen task={taskState({ done: [] })} nowMs={10 * MINUTE} />);

    expect(screen.getByText("Nothing completed yet")).toBeTruthy();
  });

  it("lists done items most recently touched first, with honest 'touched' wording", () => {
    const task = taskState({
      done: [
        itemDTO({ id: "a-1", title: "older finish", stage: "done", updatedAt: 2 * MINUTE }),
        itemDTO({ id: "a-2", title: "newer finish", stage: "done", updatedAt: 7 * MINUTE }),
      ],
    });
    render(<DoneScreen task={task} nowMs={10 * MINUTE} />);

    const titles = screen
      .getAllByText(/finish/)
      .map((element) => element.textContent);
    expect(titles).toEqual(["newer finish", "older finish"]);
    // No completion stamp exists, so the meta says "touched", never
    // "finished" — the wording is part of the contract.
    expect(screen.getByText("touched 3m ago")).toBeTruthy();
    expect(screen.getByText("touched 8m ago")).toBeTruthy();
    expect(screen.getByText("2 done")).toBeTruthy();
  });

  it("marks a pending (offline-completed) item as such", () => {
    const task = taskState({
      done: [itemDTO({ id: "a-1", stage: "done", pending: true })],
    });
    render(<DoneScreen task={task} nowMs={10 * MINUTE} />);

    expect(screen.getByText("Pending")).toBeTruthy();
  });
});
