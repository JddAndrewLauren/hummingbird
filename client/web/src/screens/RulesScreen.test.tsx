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
  ],
  alarmIntervalMs: 900_000,
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

  it("flags a rule naming a field its kind no longer declares as invalid", () => {
    render(
      <RulesScreen
        rules={[rule({ conditions: [{ field: "removed_field", op: "eq", value: "x", negate: false }] })]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
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
        kindRegistry={{
          ...registry,
          kinds: [...registry.kinds, { key: "item_threshold", mints: true, fields: [{ name: "title", fieldType: "string" }] }],
        }}
        frontier={[item]}
        lastRuleWrite={null}
        onCreateRule={vi.fn()}
        onPatchRule={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("Backtest"));

    expect(screen.getByText(/1 match — writes nothing/)).toBeTruthy();
  });

  it("creating a rule calls onCreateRule with the drafted fields", () => {
    const onCreateRule = vi.fn();
    render(
      <RulesScreen
        rules={[]}
        kindRegistry={registry}
        frontier={[]}
        lastRuleWrite={null}
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
        onCreateRule={vi.fn()}
        onPatchRule={vi.fn()}
      />,
    );
    expect(screen.getByText("boom")).toBeTruthy();
  });
});
