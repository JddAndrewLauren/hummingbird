import { useState } from "react";
import { Badge } from "../components/core/Badge";
import { Button } from "../components/core/Button";
import { Card } from "../components/core/Card";
import { EmptyState } from "../components/feedback/EmptyState";
import { Input } from "../components/forms/Input";
import { Select } from "../components/forms/Select";
import { Switch } from "../components/forms/Switch";
import type { ConditionDTO, FieldTypeName, KindRegistryDTO, RuleDTO, TaskItemDTO, TierName } from "../store/protocol";
import type { TaskState } from "../store/store";
import { backtest } from "./rules/backtest";
import { newCondition, retypeCondition, toggleNegate, widgetFor } from "./rules/condition-editor";
import { durationUnitsFor, isBelowAlarmInterval } from "./rules/duration";
import { legalOperators, OPERATOR_LABELS, type OperatorName } from "./rules/operators";
import { fieldsForKind, fieldType, kindLabel, kindOptions } from "./rules/registry";
import { invalidFields } from "./rules/validity";

// #140: the rules screen — one row per condition, enable/disable toggles,
// rule create/edit, and the backtest action. Renders entirely from the
// exported kind registry (`registry.ts`'s `kindOptions`/`fieldsForKind`) —
// never a hand-maintained field list, per ADR-0013's whole reason for
// exporting one. Voice, per the design brief: "Default-deny: what no rule
// matches stays silent."

const TIERS: TierName[] = ["urgent", "normal"];

function ConditionRow({
  condition,
  registry,
  eventKind,
  onChange,
  onRemove,
}: {
  condition: ConditionDTO;
  registry: KindRegistryDTO;
  eventKind: string | null;
  onChange: (next: ConditionDTO) => void;
  onRemove: () => void;
}) {
  const fields = fieldsForKind(registry, eventKind);
  const thisFieldType: FieldTypeName = fieldType(registry, eventKind, condition.field) ?? "string";
  const widget = widgetFor(condition.field, thisFieldType, condition.op as OperatorName);
  const alarmWarning =
    (condition.op === "within_next" || condition.op === "within_last") &&
    typeof condition.value === "string" &&
    isBelowAlarmInterval(condition.value, registry.alarmIntervalMs);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      <div style={{ display: "flex", alignItems: "flex-end", gap: "var(--space-4)", flexWrap: "wrap" }}>
        <Select
          label="Field"
          value={condition.field}
          options={fields.map((f) => ({ value: f.name, label: f.name }))}
          onChange={(e) => {
            const newType = fieldType(registry, eventKind, e.target.value) ?? "string";
            onChange({ ...retypeCondition({ ...condition, field: e.target.value }, newType), field: e.target.value });
          }}
        />
        <Select
          label="Operator"
          value={condition.op}
          options={legalOperators(thisFieldType).map((op) => ({ value: op, label: OPERATOR_LABELS[op] }))}
          onChange={(e) => onChange({ ...condition, op: e.target.value })}
        />
        <ValueWidget
          widget={widget}
          fieldType={thisFieldType}
          value={condition.value}
          onChange={(value) => onChange({ ...condition, value })}
        />
        <Switch
          label="Not"
          checked={condition.negate}
          onChange={() => onChange(toggleNegate(condition))}
        />
        <Button variant="ghost" size="sm" iconLeft="x" onClick={onRemove} aria-label="Remove condition">
          Remove
        </Button>
      </div>
      {alarmWarning ? (
        <Badge tone="warn" icon="info">
          Shorter than the sweep interval — this rule will still save, but may fire less precisely than expected.
        </Badge>
      ) : null}
    </div>
  );
}

function ValueWidget({
  widget,
  fieldType,
  value,
  onChange,
}: {
  widget: ReturnType<typeof widgetFor>;
  fieldType: FieldTypeName;
  value: unknown;
  onChange: (value: unknown) => void;
}) {
  const [chipDraft, setChipDraft] = useState("");
  if (widget === "chips") {
    const chips = Array.isArray(value) ? (value as string[]) : [];
    return (
      <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
        <span style={{ font: "var(--weight-semibold) var(--size-body-sm)/1.2 var(--font-sans)", color: "var(--text-secondary)" }}>
          Value
        </span>
        <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
          {chips.map((chip, i) => (
            <Badge key={`${chip}-${i}`} tone="neutral">
              {chip}
              <button
                type="button"
                aria-label={`Remove ${chip}`}
                onClick={() => onChange(chips.filter((_, idx) => idx !== i))}
                style={{ marginLeft: 4, background: "none", border: "none", cursor: "pointer" }}
              >
                ×
              </button>
            </Badge>
          ))}
        </div>
        <Input
          value={chipDraft}
          placeholder="Add value, press Enter"
          onChange={(e) => setChipDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && chipDraft.trim() !== "") {
              e.preventDefault();
              onChange([...chips, chipDraft.trim()]);
              setChipDraft("");
            }
          }}
        />
      </div>
    );
  }
  if (widget === "duration" || widget === "datetime") {
    const text = typeof value === "string" ? value : "";
    const units = durationUnitsFor(fieldType === "date" ? "date" : "timestamp");
    const match = /^(\d+)([mhd])$/.exec(text);
    const amount = match ? match[1] : "";
    const unit = match ? match[2] : units[0];
    return (
      <div style={{ display: "flex", gap: "var(--space-2)", alignItems: "flex-end" }}>
        <Input
          label={widget === "datetime" ? "Within" : "Duration"}
          value={amount}
          style={{ width: 80 }}
          onChange={(e) => onChange(`${e.target.value.replace(/\D/g, "")}${unit}`)}
        />
        <Select
          value={unit}
          options={units.map((u) => ({ value: u, label: u }))}
          onChange={(e) => onChange(`${amount}${e.target.value}`)}
        />
      </div>
    );
  }
  if (widget === "boolean") {
    return (
      <Switch label="Value" checked={value === true} onChange={(e) => onChange(e.target.checked)} />
    );
  }
  if (widget === "number") {
    return (
      <Input
        label="Value"
        type="number"
        value={typeof value === "number" ? String(value) : ""}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    );
  }
  return (
    <Input
      label="Value"
      value={typeof value === "string" ? value : ""}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

function BacktestPanel({ rule, items }: { rule: Pick<RuleDTO, "eventKind" | "conditions">; items: TaskItemDTO[] }) {
  const [result, setResult] = useState<ReturnType<typeof backtest> | null>(null);
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      <Button variant="secondary" size="sm" iconLeft="search" onClick={() => setResult(backtest(rule, items, Date.now()))}>
        Backtest
      </Button>
      {result?.kind === "ok" ? (
        <span className="hb-meta">
          {result.matches.length} {result.matches.length === 1 ? "match" : "matches"} — writes nothing
        </span>
      ) : null}
      {result?.kind === "unavailable" ? (
        <span className="hb-meta">No local event history to backtest this kind against.</span>
      ) : null}
      {result?.kind === "ok" && result.matches.length > 0 ? (
        <ul style={{ margin: 0, paddingLeft: "var(--space-6)" }}>
          {result.matches.map((item) => (
            <li key={item.id} style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
              {item.title}
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

interface RuleEditorState {
  name: string;
  eventKind: string | null;
  conditions: ConditionDTO[];
  severity: string;
  tier: TierName;
  enabled: boolean;
}

function emptyEditorState(): RuleEditorState {
  return { name: "", eventKind: null, conditions: [], severity: "normal", tier: "normal", enabled: true };
}

function editorStateFromRule(rule: RuleDTO): RuleEditorState {
  return {
    name: rule.name,
    eventKind: rule.eventKind,
    conditions: rule.conditions,
    severity: rule.severity,
    tier: rule.tier,
    enabled: rule.enabled,
  };
}

function RuleEditorForm({
  registry,
  state,
  onChange,
  onSave,
  saveLabel,
}: {
  registry: KindRegistryDTO;
  state: RuleEditorState;
  onChange: (next: RuleEditorState) => void;
  onSave: () => void;
  saveLabel: string;
}) {
  const fields = fieldsForKind(registry, state.eventKind);
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
      <div style={{ display: "flex", gap: "var(--space-4)", flexWrap: "wrap" }}>
        <Input label="Name" value={state.name} onChange={(e) => onChange({ ...state, name: e.target.value })} />
        <Select
          label="Kind"
          value={state.eventKind ?? ""}
          options={kindOptions(registry).map((k) => ({ value: k.key ?? "", label: k.label }))}
          onChange={(e) => {
            const eventKind = e.target.value === "" ? null : e.target.value;
            onChange({ ...state, eventKind, conditions: [] });
          }}
        />
        <Input label="Severity" value={state.severity} onChange={(e) => onChange({ ...state, severity: e.target.value })} />
        <Select
          label="Tier"
          value={state.tier}
          options={TIERS.map((t) => ({ value: t, label: t }))}
          onChange={(e) => onChange({ ...state, tier: e.target.value as TierName })}
        />
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
        <span className="hb-meta">conditions</span>
        {state.conditions.map((condition, i) => (
          <ConditionRow
            key={i}
            condition={condition}
            registry={registry}
            eventKind={state.eventKind}
            onChange={(next) => {
              const conditions = [...state.conditions];
              conditions[i] = next;
              onChange({ ...state, conditions });
            }}
            onRemove={() => onChange({ ...state, conditions: state.conditions.filter((_, idx) => idx !== i) })}
          />
        ))}
        <Button
          variant="ghost"
          size="sm"
          iconLeft="plus"
          disabled={fields.length === 0}
          onClick={() => {
            const field = fields[0];
            onChange({ ...state, conditions: [...state.conditions, newCondition(field.name, field.fieldType)] });
          }}
        >
          Add condition
        </Button>
      </div>
      <Button variant="primary" onClick={onSave} disabled={state.name.trim() === ""}>
        {saveLabel}
      </Button>
    </div>
  );
}

function RuleCard({
  rule,
  registry,
  items,
  onToggle,
  onSave,
}: {
  rule: RuleDTO;
  registry: KindRegistryDTO;
  items: TaskItemDTO[];
  onToggle: (enabled: boolean) => void;
  onSave: (state: RuleEditorState) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<RuleEditorState>(() => editorStateFromRule(rule));
  const invalid = invalidFields(rule, registry);

  return (
    <Card padding="var(--space-5)" style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-4)" }}>
        <div style={{ display: "flex", alignItems: "center", gap: "var(--space-3)" }}>
          <span style={{ font: "var(--type-body-strong)" }}>{rule.name}</span>
          <Badge tone={rule.tier === "urgent" ? "danger" : "info"} mono>
            {rule.tier}
          </Badge>
          <Badge tone="neutral" mono>
            {rule.eventKind === null ? "any kind" : kindLabel(rule.eventKind)}
          </Badge>
          {invalid.length > 0 ? (
            <Badge tone="danger" icon="info">
              Invalid — names a field its kind no longer declares
            </Badge>
          ) : null}
        </div>
        <Switch checked={rule.enabled} onChange={(e) => onToggle(e.target.checked)} label="Enabled" />
      </div>
      <BacktestPanel rule={rule} items={items} />
      <Button variant="ghost" size="sm" onClick={() => setEditing((v) => !v)}>
        {editing ? "Close" : "Edit"}
      </Button>
      {editing ? (
        <RuleEditorForm
          registry={registry}
          state={draft}
          onChange={setDraft}
          onSave={() => {
            onSave(draft);
            setEditing(false);
          }}
          saveLabel="Save changes"
        />
      ) : null}
    </Card>
  );
}

export interface RulesScreenProps {
  rules: RuleDTO[] | null;
  kindRegistry: KindRegistryDTO | null;
  frontier: TaskItemDTO[];
  lastRuleWrite: TaskState["lastRuleWrite"];
  onCreateRule: (
    name: string,
    eventKind: string | null,
    conditions: ConditionDTO[],
    severity: string,
    tier: TierName,
    enabled: boolean,
  ) => void;
  onPatchRule: (
    current: RuleDTO,
    patch: {
      name?: string | null;
      eventKind?: string | null;
      conditions?: ConditionDTO[] | null;
      severity?: string | null;
      tier?: TierName | null;
      enabled?: boolean | null;
    },
  ) => void;
}

export function RulesScreen({
  rules,
  kindRegistry,
  frontier,
  lastRuleWrite,
  onCreateRule,
  onPatchRule,
}: RulesScreenProps) {
  const [creating, setCreating] = useState(false);
  const [draft, setDraft] = useState<RuleEditorState>(emptyEditorState);

  if (kindRegistry === null || rules === null) {
    return <Card padding="var(--space-6)">Loading rules…</Card>;
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}>
      <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between" }}>
        <h3 style={{ font: "var(--type-h3)" }}>Rules</h3>
        <span className="hb-meta">{rules.length} rules · default-deny</span>
      </div>
      {lastRuleWrite !== null && lastRuleWrite.kind !== "ok" ? (
        <Badge tone="danger">{lastRuleWrite.error ?? "That rule write did not go through."}</Badge>
      ) : null}
      {rules.length === 0 ? (
        <Card padding="0">
          <EmptyState
            icon="siren"
            headingLevel={3}
            title="No rules yet"
            body="What no rule matches stays silent."
          />
        </Card>
      ) : (
        rules.map((rule) => (
          <RuleCard
            key={rule.id}
            rule={rule}
            registry={kindRegistry}
            items={frontier}
            onToggle={(enabled) => onPatchRule(rule, { enabled })}
            onSave={(state) =>
              onPatchRule(rule, {
                name: state.name,
                eventKind: state.eventKind,
                conditions: state.conditions,
                severity: state.severity,
                tier: state.tier,
                enabled: state.enabled,
              })
            }
          />
        ))
      )}
      <Button variant="secondary" iconLeft="plus" onClick={() => setCreating((v) => !v)}>
        {creating ? "Cancel" : "New rule"}
      </Button>
      {creating ? (
        <Card padding="var(--space-5)">
          <RuleEditorForm
            registry={kindRegistry}
            state={draft}
            onChange={setDraft}
            onSave={() => {
              onCreateRule(draft.name, draft.eventKind, draft.conditions, draft.severity, draft.tier, draft.enabled);
              setDraft(emptyEditorState());
              setCreating(false);
            }}
            saveLabel="Create rule"
          />
        </Card>
      ) : null}
    </div>
  );
}
