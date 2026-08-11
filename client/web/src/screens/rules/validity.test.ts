import { describe, expect, it } from "vitest";
import type { KindRegistryDTO, RuleDTO } from "../../store/protocol";
import { invalidFields, isRuleValid } from "./validity";

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
    name: "test rule",
    eventKind: "email",
    conditions: [],
    severity: "high",
    tier: "urgent",
    enabled: true,
    updatedAt: 1,
    version: 1,
    ...overrides,
  };
}

describe("invalidFields / isRuleValid", () => {
  it("is valid when every condition names a field its kind still declares", () => {
    const r = rule({ conditions: [{ field: "subject", op: "contains", value: "x", negate: false }] });
    expect(invalidFields(r, registry)).toEqual([]);
    expect(isRuleValid(r, registry)).toBe(true);
  });

  it("is valid against a core field even without a kind-specific match", () => {
    const r = rule({ conditions: [{ field: "source", op: "eq", value: "x", negate: false }] });
    expect(isRuleValid(r, registry)).toBe(true);
  });

  it("flags a condition naming a field its kind no longer declares", () => {
    const r = rule({
      conditions: [{ field: "removed_field", op: "eq", value: "x", negate: false }],
    });
    expect(invalidFields(r, registry)).toEqual([{ field: "removed_field" }]);
    expect(isRuleValid(r, registry)).toBe(false);
  });

  it("narrows to core fields for an any-kind rule, flagging a kind-only field", () => {
    const r = rule({
      eventKind: null,
      conditions: [{ field: "subject", op: "contains", value: "x", negate: false }],
    });
    expect(isRuleValid(r, registry)).toBe(false);
  });
});
