// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { ledgerRowDTO, render, screen, taskState } from "../test/component";
import { LedgerScreen } from "./LedgerScreen";

const MINUTE = 60_000;

describe("LedgerScreen", () => {
  it("renders 'not read yet' while the ledger is null — an empty roster is a claim", () => {
    render(<LedgerScreen task={taskState()} nowMs={10 * MINUTE} />);

    expect(screen.getByText("not read yet")).toBeTruthy();
    expect(screen.queryByText("Nothing has ever been tracked")).toBeNull();
  });

  it("renders the empty state only for a real, empty answer", () => {
    render(<LedgerScreen task={taskState({ ledger: [] })} nowMs={10 * MINUTE} />);

    expect(screen.getByText("Nothing has ever been tracked")).toBeTruthy();
  });

  it("shows every row — live, done and archived — with the archived one labelled", () => {
    const task = taskState({
      ledger: [
        ledgerRowDTO({ id: "a-1", title: "still open", updatedAt: 8 * MINUTE }),
        ledgerRowDTO({ id: "a-2", title: "finished thing", stage: "done", updatedAt: 5 * MINUTE }),
        ledgerRowDTO({
          id: "a-3",
          title: "cancelled thing",
          archivedAt: 6 * MINUTE,
          absentSinceMs: 6 * MINUTE,
        }),
      ],
    });
    render(<LedgerScreen task={task} nowMs={10 * MINUTE} />);

    expect(screen.getByText("still open")).toBeTruthy();
    expect(screen.getByText("finished thing")).toBeTruthy();
    expect(screen.getByText("cancelled thing")).toBeTruthy();
    expect(screen.getByText("archived 4m ago")).toBeTruthy();
    expect(screen.getByText("3 ever · derived, not recorded")).toBeTruthy();
  });

  it("carries the three badges only on rows that earn them", () => {
    const task = taskState({
      ledger: [
        ledgerRowDTO({ id: "a-1", title: "quiet row" }),
        ledgerRowDTO({
          id: "a-2",
          title: "busy row",
          pending: true,
          deadLettered: true,
          hasLiveAlert: true,
        }),
      ],
    });
    render(<LedgerScreen task={task} nowMs={10 * MINUTE} />);

    expect(screen.getAllByText("Pending")).toHaveLength(1);
    expect(screen.getAllByText("edit didn't apply")).toHaveLength(1);
    expect(screen.getAllByText("alert")).toHaveLength(1);
  });
});
