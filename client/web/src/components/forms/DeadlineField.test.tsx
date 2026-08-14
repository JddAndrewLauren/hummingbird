// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DeadlineField } from "./DeadlineField";

afterEach(cleanup);

function renderField(value: string) {
  const onChange = vi.fn();
  const view = render(<DeadlineField value={value} onChange={onChange} />);
  return { onChange, view };
}

const date = () => screen.getByLabelText("Deadline") as HTMLInputElement;
const time = () => screen.getByLabelText("Time") as HTMLInputElement;

describe("DeadlineField", () => {
  it("rests as a date alone, with the time behind a deliberate second gesture", () => {
    renderField("");

    expect(date().value).toBe("");
    expect(screen.queryByLabelText("Time")).toBeNull();
    expect(screen.getByRole("button", { name: "Add time" })).toBeDefined();
  });

  it("shows the time picker unasked when the deadline already names a minute", () => {
    renderField("2026-09-01T09:30");

    expect(date().value).toBe("2026-09-01");
    expect(time().value).toBe("09:30");
    // The revealing button would be a second way to say what is already true.
    expect(screen.queryByRole("button", { name: "Add time" })).toBeNull();
  });

  it("sends one joined value when the date changes, keeping the time", () => {
    const { onChange } = renderField("2026-09-01T09:30");

    fireEvent.change(date(), { target: { value: "2026-09-02" } });

    expect(onChange).toHaveBeenCalledWith("2026-09-02T09:30");
  });

  it("joins a newly picked time onto the day", () => {
    const { onChange } = renderField("2026-09-01");

    fireEvent.click(screen.getByRole("button", { name: "Add time" }));
    fireEvent.change(time(), { target: { value: "17:45" } });

    expect(onChange).toHaveBeenCalledWith("2026-09-01T17:45");
  });

  it("puts the deadline back to a whole day when the time is removed", () => {
    // Not to midnight: a deadline with no hour is a different fact from one at
    // 00:00, and the × means the former.
    const { onChange } = renderField("2026-09-01T09:30");

    fireEvent.click(screen.getByRole("button", { name: "Remove the time" }));

    expect(onChange).toHaveBeenCalledWith("2026-09-01");
  });

  it("clears the whole value when the date is cleared, time included", () => {
    const { onChange } = renderField("2026-09-01T09:30");

    fireEvent.change(date(), { target: { value: "" } });

    expect(onChange).toHaveBeenCalledWith("");
  });

  it("shows an error against the date, where the field's own message belongs", () => {
    render(<DeadlineField value="" onChange={() => {}} error="Use YYYY-MM-DD" />);

    expect(screen.getByText("Use YYYY-MM-DD")).toBeDefined();
    expect(date().getAttribute("aria-invalid")).toBe("true");
  });
});
