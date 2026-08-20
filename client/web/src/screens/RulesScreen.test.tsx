// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import type { KindRegistryDTO, RuleDTO } from "../store/protocol";
import { fireEvent, itemDTO, render, screen } from "../test/component";
import { RulesScreen } from "./RulesScreen";

const registry: KindRegistryDTO = {
  coreFields: [{ name: "source", fieldType: "string" }],
  kinds: [
    {
      key: "email",
      mints: true,
      fields: [{ name: "subject", fieldType: "string" }],
    },
    {
      key: "item_threshold",
      mints: true,
      fields: [
        { name: "title", fieldType: "string" },
        { name: "deadline", fieldType: "timestamp" },
      ],
    },
  ],
  alarmIntervalMs: 900_000,
  severities: ["low", "normal", "high", "urgent"],
};

function rule(overrides: Partial<RuleDTO> = {}): RuleDTO {
  return {
    id: "r-1",
    name: "trash slide",
    eventKind: "email",
    conditions: [{ field: "subject", op: "contains", value: "urgent", negate: false }],
    severity: "high",
    tier: "urgent",
    enabled: true,
    updatedAt: 1,
    version: 3,
    ...overrides,
  };
}

describe("RulesScreen", () => {
  it("shows a loading state until both rules and the kind registry have arrived", () => {
    render(
      <RulesScreen
        rules={null}
        kindRegistry={null}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={vi.fn()}
        onPatchRule={vi.fn()}
      />,
    );
    expect(screen.getByText(/Loading rules/)).toBeTruthy();
  });

  it("renders an empty state with the default-deny copy when there are no rules", () => {
    render(
      <RulesScreen
        rules={[]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={vi.fn()}
        onPatchRule={vi.fn()}
      />,
    );
    expect(screen.getByText("No rules yet")).toBeTruthy();
    expect(screen.getByText(/what no rule matches stays silent/i)).toBeTruthy();
  });

  it("renders every rule from props, with its kind resolved from the registry", () => {
    render(
      <RulesScreen
        rules={[rule()]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={vi.fn()}
        onPatchRule={vi.fn()}
      />,
    );
    expect(screen.getByText("trash slide")).toBeTruthy();
    expect(screen.getByText("Email")).toBeTruthy();
  });

  it("toggling enabled calls onPatchRule with only the enabled field touched", () => {
    const onPatchRule = vi.fn();
    render(
      <RulesScreen
        rules={[rule({ enabled: true })]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={vi.fn()}
        onPatchRule={onPatchRule}
      />,
    );

    fireEvent.click(screen.getByLabelText("Enabled"));

    expect(onPatchRule).toHaveBeenCalledTimes(1);
    const [current, patch] = onPatchRule.mock.calls[0];
    expect(current.id).toBe("r-1");
    expect(patch).toEqual({ enabled: false });
  });

  it("shows the switch's clicked value as pending until a cycle completes, rather than reverting", () => {
    const onPatchRule = vi.fn();
    const { rerender } = render(
      <RulesScreen
        rules={[rule({ enabled: true })]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={vi.fn()}
        onPatchRule={onPatchRule}
      />,
    );

    fireEvent.click(screen.getByLabelText("Enabled"));

    // The click is only queued — `rules` still reports `enabled: true` — but
    // the rendered switch must show the clicked value, not revert to it,
    // and say it is pending.
    const toggle = screen.getByLabelText("Enabled") as HTMLInputElement;
    expect(toggle.checked).toBe(false);
    expect(screen.getByText("pending")).toBeTruthy();

    // A completed cycle (syncOutcomeSeq bumps) with the write now landed
    // clears the pending state — the rendered value tracks the real rule
    // again, with no separate confirmation step required.
    rerender(
      <RulesScreen
        rules={[rule({ enabled: false })]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={1}
        onCreateRule={vi.fn()}
        onPatchRule={onPatchRule}
      />,
    );

    expect((screen.getByLabelText("Enabled") as HTMLInputElement).checked).toBe(false);
    expect(screen.queryByText("pending")).toBeNull();
  });

  it("reseeds a stale editor draft when the stored rule moves underneath it, so Save cannot push old values back", () => {
    const onPatchRule = vi.fn();
    const { rerender } = render(
      <RulesScreen
        rules={[rule({ enabled: true })]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={vi.fn()}
        onPatchRule={onPatchRule}
      />,
    );

    // Toggle enabled off and let the (simulated) cycle land — the rule prop
    // now reports enabled: false.
    fireEvent.click(screen.getByLabelText("Enabled"));
    rerender(
      <RulesScreen
        rules={[rule({ enabled: false })]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={1}
        onCreateRule={vi.fn()}
        onPatchRule={onPatchRule}
      />,
    );

    // Opening the editor now and saving with no further changes must not
    // resurrect the mount-time (enabled: true) draft.
    fireEvent.click(screen.getByText("Edit"));
    fireEvent.click(screen.getByText("Save changes"));

    const patch = onPatchRule.mock.calls[onPatchRule.mock.calls.length - 1][1];
    expect(patch.enabled).toBe(false);
  });

  it("flags a rule naming a field its kind no longer declares as invalid", () => {
    render(
      <RulesScreen
        rules={[rule({ conditions: [{ field: "removed_field", op: "eq", value: "x", negate: false }] })]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={vi.fn()}
        onPatchRule={vi.fn()}
      />,
    );
    expect(screen.getByText(/Invalid/)).toBeTruthy();
  });

  it("running backtest against an item_threshold rule shows a match count and writes nothing", () => {
    const item = itemDTO({ id: "i-1", title: "renew passport" });
    render(
      <RulesScreen
        rules={[
          rule({
            eventKind: "item_threshold",
            conditions: [{ field: "title", op: "contains", value: "passport", negate: false }],
          }),
        ]}
        kindRegistry={registry}
        frontier={[item]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={vi.fn()}
        onPatchRule={vi.fn()}
      />,
    );

    // #140 review: backtest lives in the editor now, not on the plain card.
    fireEvent.click(screen.getByText("Edit"));
    fireEvent.click(screen.getByText("Backtest"));

    expect(screen.getByText(/1 of 1 actionable item would match — writes nothing/)).toBeTruthy();
  });

  it("backtest is available on a draft BEFORE it is ever saved — the create form, not just the edit form", () => {
    const item = itemDTO({ id: "i-1", title: "renew passport" });
    const onCreateRule = vi.fn();
    render(
      <RulesScreen
        rules={[]}
        kindRegistry={registry}
        frontier={[item]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={onCreateRule}
        onPatchRule={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("New rule"));
    fireEvent.change(screen.getByLabelText("Kind"), { target: { value: "item_threshold" } });
    fireEvent.click(screen.getByText("Add condition"));
    fireEvent.change(screen.getByLabelText("Field"), { target: { value: "title" } });
    fireEvent.change(screen.getByLabelText("Operator"), { target: { value: "contains" } });
    fireEvent.change(screen.getByLabelText("Value"), { target: { value: "passport" } });

    fireEvent.click(screen.getByText("Backtest"));

    expect(screen.getByText(/1 of 1 actionable item would match — writes nothing/)).toBeTruthy();
    // Never saved — the count is visible with no write having happened.
    expect(onCreateRule).not.toHaveBeenCalled();
  });

  it("severity is a select over the domain vocabulary, never free text", () => {
    render(
      <RulesScreen
        rules={[]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={vi.fn()}
        onPatchRule={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("New rule"));

    const severitySelect = screen.getByLabelText("Severity") as HTMLSelectElement;
    expect(Array.from(severitySelect.options).map((o) => o.value)).toEqual(registry.severities);
  });

  it("marks a rule stored at a severity outside the registry as unranked, on the card and in the editor", () => {
    render(
      <RulesScreen
        rules={[rule({ severity: "warning" })]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={vi.fn()}
        onPatchRule={vi.fn()}
      />,
    );

    // Visible on the closed card, with the cost spelled out — not just
    // that the value is unusual.
    expect(screen.getByText(/unranked/i)).toBeTruthy();
    expect(screen.getByText(/loses/i)).toBeTruthy();
    expect(screen.getByText(/escalate/i)).toBeTruthy();

    // And again in the editor, next to the severity select itself.
    fireEvent.click(screen.getByText("Edit"));
    expect(screen.getAllByText(/unranked/i).length).toBeGreaterThan(1);
  });

  it("renders the invalid and unranked badges together when a rule is both, neither suppressing the other", () => {
    render(
      <RulesScreen
        rules={[
          rule({
            severity: "warning",
            conditions: [{ field: "removed_field", op: "eq", value: "x", negate: false }],
          }),
        ]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={vi.fn()}
        onPatchRule={vi.fn()}
      />,
    );

    // Both badges, with their own copy: the two facts are independent —
    // "this condition could not be written today" and "this severity ranks
    // below every declared one" — so neither is allowed to stand in for the
    // other, and neither early-returns past it.
    const invalid = screen.getByText("Invalid — names a field its kind no longer declares");
    const unranked = screen.getByText(/^Unranked severity —/);
    expect(invalid.textContent).toContain("no longer declares");
    expect(unranked.textContent).toContain("can never escalate a notification");

    // And at their own tones — danger for the broken rule, warn for the
    // working-but-outranked one. A single tone for both would say the
    // unranked rule is broken, which it is not (#338).
    const badge = (labelEl: HTMLElement) => labelEl.parentElement?.getAttribute("style") ?? "";
    expect(badge(invalid)).toContain("--status-danger-fg");
    expect(badge(unranked)).toContain("--status-warn-fg");

    // Same row, in the card's badge group, rather than one of them landing
    // somewhere else on the card.
    expect(invalid.parentElement?.parentElement).toBe(unranked.parentElement?.parentElement);
  });

  it("keeps sentence-length badges out of the non-wrapping identity row (#374)", () => {
    render(
      <RulesScreen
        rules={[
          rule({
            severity: "warning",
            conditions: [{ field: "removed_field", op: "eq", value: "x", negate: false }],
          }),
        ]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={vi.fn()}
        onPatchRule={vi.fn()}
      />,
    );

    const invalid = screen.getByText("Invalid — names a field its kind no longer declares");
    const unranked = screen.getByText(/^Unranked severity —/);
    const badgeRow = invalid.parentElement?.parentElement as HTMLElement;
    // A wrapping row of its own, distinct from the mono tier/kind chips'
    // identity row — the fixed-height, non-wrapping row is never asked to
    // hold a sentence.
    expect(badgeRow.getAttribute("style")).toContain("flex-wrap: wrap");

    const tierChip = screen.getByText("urgent");
    const identityRow = tierChip.parentElement as HTMLElement;
    expect(identityRow).not.toBe(badgeRow);
    expect(identityRow.contains(invalid)).toBe(false);
    expect(identityRow.contains(unranked)).toBe(false);
  });

  it("does not mark a rule at a declared severity as unranked", () => {
    render(
      <RulesScreen
        rules={[rule({ severity: "high" })]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={vi.fn()}
        onPatchRule={vi.fn()}
      />,
    );

    expect(screen.queryByText(/unranked/i)).toBeNull();
    fireEvent.click(screen.getByText("Edit"));
    expect(screen.queryByText(/unranked/i)).toBeNull();
  });

  it("does not mark a rule stored at the empty-string severity as unranked", () => {
    render(
      <RulesScreen
        rules={[rule({ severity: "" })]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={vi.fn()}
        onPatchRule={vi.fn()}
      />,
    );

    expect(screen.queryByText(/unranked/i)).toBeNull();
    fireEvent.click(screen.getByText("Edit"));
    expect(screen.queryByText(/unranked/i)).toBeNull();
  });

  it("offers a date/time picker for deadline, never the plain duration control", () => {
    render(
      <RulesScreen
        rules={[]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={vi.fn()}
        onPatchRule={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("New rule"));
    fireEvent.change(screen.getByLabelText("Kind"), { target: { value: "item_threshold" } });
    fireEvent.click(screen.getByText("Add condition"));
    fireEvent.change(screen.getByLabelText("Field"), { target: { value: "deadline" } });

    const deadlineInput = screen.getByLabelText("Deadline") as HTMLInputElement;
    expect(deadlineInput.type).toBe("datetime-local");
  });

  it("creating a rule calls onCreateRule with the drafted fields", () => {
    const onCreateRule = vi.fn();
    render(
      <RulesScreen
        rules={[]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={onCreateRule}
        onPatchRule={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("New rule"));
    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "trash slide" } });
    fireEvent.click(screen.getByText("Create rule"));

    expect(onCreateRule).toHaveBeenCalledTimes(1);
    expect(onCreateRule).toHaveBeenCalledWith("trash slide", null, [], "normal", "normal", true);
  });

  it("selecting any kind narrows the field list to the Event core", () => {
    render(
      <RulesScreen
        rules={[]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
        syncOutcomeSeq={0}
        onCreateRule={vi.fn()}
        onPatchRule={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("New rule"));
    fireEvent.click(screen.getByText("Add condition"));

    const fieldSelect = screen.getByLabelText("Field") as HTMLSelectElement;
    const optionValues = Array.from(fieldSelect.options).map((o) => o.value);
    expect(optionValues).toEqual(["source"]);
  });

  it("surfaces the most recent failed rule write", () => {
    render(
      <RulesScreen
        rules={[rule()]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={{ seed: "s-1", ruleId: "r-1", kind: "failed", error: "boom" }}
        syncOutcomeSeq={0}
        onCreateRule={vi.fn()}
        onPatchRule={vi.fn()}
      />,
    );
    expect(screen.getByText("boom")).toBeTruthy();
  });
});
