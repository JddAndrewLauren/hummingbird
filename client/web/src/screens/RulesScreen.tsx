import { useState } from "react";
import { Badge } from "../components/core/Badge";
import { Button } from "../components/core/Button";
import { Card } from "../components/core/Card";
import { EmptyState } from "../components/feedback/EmptyState";
import { Input } from "../components/forms/Input";
import { Select } from "../components/forms/Select";
import { Switch } from "../components/forms/Switch";
import type {
  ConditionDTO,
  FieldTypeName,
  KindRegistryDTO,
  RuleDTO,
  SourceOptionDTO,
  TaskItemDTO,
  TierName,
} from "../store/protocol";
import type { TaskState } from "../store/store";
import { defaultSeverity } from "../decisions/seam";
import { backtest } from "./rules/backtest";
import { newCondition, retypeCondition, toggleNegate, widgetFor } from "./rules/condition-editor";
import { datetimeInputValueFromDuration, durationFromDatetimeInputValue, type DeadlineOperator } from "./rules/deadline-picker";
import { durationUnitsFor, formatDuration, isBelowAlarmInterval, type DurationUnit } from "./rules/duration";
import { legalOperators, OPERATOR_LABELS, type OperatorName } from "./rules/operators";
import { fieldsForKind, fieldType, kindLabel, kindOptions } from "./rules/registry";
import { isRuleValid } from "./rules/validity";

// #140: the rules screen — one row per condition, enable/disable toggles,
// rule create/edit, and the backtest action. Renders entirely from the
// exported kind registry (`registry.ts`'s `kindOptions`/`fieldsForKind`) —
// never a hand-maintained field list, per ADR-0013's whole reason for
// exporting one. Voice, per the design brief: "Default-deny: what no rule
// matches stays silent."

const TIERS: TierName[] = ["urgent", "normal"];

/** A Select's options, with `current` prepended (labelled as itself) when it
 * is not already among `known` — the field-select and severity-select
 * fallback for a value this build's registry no longer declares. Without
 * this a `<Select value={current}>` whose `current` matches no `<option>`
 * silently shows the browser's own blank/first-item behaviour rather than
 * naming the value actually stored. */
function withCurrentOption(known: string[], current: string): string[] {
  return known.includes(current) || current === "" ? known : [current, ...known];
}

/** Whether `current` is a severity this build's registry does not declare —
 * the same condition `withCurrentOption` already detects (and special-cases
 * for the empty string, which is not a stamped value to warn about), spent
 * here on telling the reader rather than only avoiding a blank `<select>`.
 * `hummingbird_domain::severity_rank` (#335/#338) ranks such a value `0`:
 * below every declared severity, so it loses every mint fold and can never
 * ring an escalation — a legal, working rule, not a validation error. */
function isUnrankedSeverity(known: string[], current: string): boolean {
  return current !== "" && !known.includes(current);
}

const UNRANKED_SEVERITY_COPY =
  "Unranked severity — loses every fold against a declared severity, and can never escalate a notification.";

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
  const fieldOptions = withCurrentOption(
    fields.map((f) => f.name),
    condition.field,
  );

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      <div style={{ display: "flex", alignItems: "flex-end", gap: "var(--space-4)", flexWrap: "wrap" }}>
        <Select
          label="Field"
          value={condition.field}
          options={fieldOptions.map((name) => ({ value: name, label: name }))}
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
          op={condition.op as OperatorName}
          sources={registry.sources}
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
  op,
  sources,
  value,
  onChange,
}: {
  widget: ReturnType<typeof widgetFor>;
  fieldType: FieldTypeName;
  op: OperatorName;
  sources: SourceOptionDTO[];
  value: unknown;
  onChange: (value: unknown) => void;
}) {
  const [chipDraft, setChipDraft] = useState("");
  // The datetime picker's anchor clock — read once, at mount, via a lazy
  // initializer rather than a bare `Date.now()` in the render body (React's
  // purity rule: a component must render the same output for the same
  // props/state). `ConditionRow` keys each row by index, so this remounts
  // — and re-reads the clock — whenever a condition is added, removed, or
  // the editor is reopened fresh; good enough for a picker's own moment,
  // never a stored value anything must stay byte-identical against.
  const [nowMs] = useState(() => Date.now());
  if (widget === "source") {
    // `source`'s legal values are a frozen registry (ADR-0014), so this is
    // a pick, never a text box: a typo produces a rule that matches nothing
    // and says nothing about it. Which conditions get this control is
    // `rules::widget_for`'s answer (`eq` only — `contains` is substring
    // matching, which a whole-value picker cannot express).
    const text = typeof value === "string" ? value : "";
    // A retired source still renders — an existing rule may name one —
    // but cannot be newly picked: the authority already 400s that save
    // (`RuleProblem::RetiredSource`), and this is where the operator finds
    // out first. `withCurrentOption` covers the other direction: a stored
    // value this build's registry does not declare at all still names
    // itself rather than showing the browser's blank/first-item behaviour.
    const known = sources.map((s) => s.source);
    const options = withCurrentOption(known, text).map((source) => {
      const retiredAs = sources.find((s) => s.source === source)?.retiredAs ?? null;
      if (retiredAs === null) {
        return { value: source, label: source };
      }
      return {
        value: source,
        label: `${source} — retired, use ${retiredAs}`,
        // Not disabled when it is the value already stored: a `<select>`
        // whose selected option is disabled reads as nothing selected, and
        // the row would stop naming the source the rule actually carries.
        disabled: source !== text,
      };
    });
    return (
      <Select
        label="Source"
        value={text}
        // A fresh condition starts empty, which is no source at all — and a
        // `<select>` with no matching option shows its first entry while
        // the state stays "", which would read as a source already chosen.
        // The placeholder is what keeps the control honest about that.
        options={text === "" ? [{ value: "", label: "Pick a source…" }, ...options] : options}
        onChange={(e) => onChange(e.target.value)}
      />
    );
  }
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
                style={{ marginLeft: "var(--space-2)", background: "none", border: "none", cursor: "pointer" }}
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
  if (widget === "datetime") {
    // Acceptance criterion 3: "date/time for `deadline`". The picker
    // renders a concrete moment; `deadline-picker.ts` is what converts that
    // moment to and from the wire's relative-duration value, against
    // `Date.now()` — the same ephemeral, caller-side clock `BacktestPanel`
    // already reads directly, since nothing here is a stored value that
    // could later disagree with itself (`alert::plan`'s own concern does
    // not apply to a picker, only to a written string).
    const text = typeof value === "string" ? value : "";
    const deadlineOp: DeadlineOperator = op === "within_last" ? "within_last" : "within_next";
    return (
      <Input
        label="Deadline"
        type="datetime-local"
        value={datetimeInputValueFromDuration(text, deadlineOp, nowMs)}
        onChange={(e) => {
          const duration = durationFromDatetimeInputValue(e.target.value, deadlineOp, nowMs);
          if (duration !== undefined) {
            onChange(duration);
          }
        }}
      />
    );
  }
  if (widget === "duration") {
    const text = typeof value === "string" ? value : "";
    const units = durationUnitsFor(fieldType === "date" ? "date" : "timestamp");
    const match = /^(\d+)([mhd])$/.exec(text);
    const amount = match ? match[1] : "";
    const unit = (match ? match[2] : units[0]) as DurationUnit;
    return (
      <div style={{ display: "flex", gap: "var(--space-2)", alignItems: "flex-end" }}>
        <Input
          label="Duration"
          value={amount}
          style={{ width: "var(--space-13)" }}
          onChange={(e) => onChange(formatDuration(Number(e.target.value.replace(/\D/g, "")) || 0, unit))}
        />
        <Select
          value={unit}
          options={units.map((u) => ({ value: u, label: u }))}
          onChange={(e) => onChange(formatDuration(Number(amount) || 0, e.target.value as DurationUnit))}
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

/** Backtest (ADR-0011): "the match count is shown before save." Lives
 * inside the editor, driven by the DRAFT being edited — never the last
 * saved row — which is what makes the count available before a create or
 * an edit is saved at all, closing the gap #140's review found (a draft's
 * first tick would otherwise fire with nobody having seen a count). */
function BacktestPanel({ rule, items }: { rule: Pick<RuleDTO, "eventKind" | "conditions">; items: TaskItemDTO[] }) {
  const [result, setResult] = useState<ReturnType<typeof backtest> | null>(null);
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      <span className="hb-meta">backtest</span>
      <Button variant="secondary" size="sm" iconLeft="search" onClick={() => setResult(backtest(rule, items, Date.now()))}>
        Backtest
      </Button>
      {result?.kind === "ok" ? (
        <>
          <span className="hb-meta">
            {result.matches.length} of {items.length} actionable {items.length === 1 ? "item" : "items"} would
            match — writes nothing
          </span>
          {/* Honesty over a bare count (#140 review): this checks the same
              corpus the Now screen's frontier shows — Ready or In Progress,
              unblocked. `sweep_tick` itself evaluates every open item,
              including Triage and blocked ones, which this check cannot
              see. */}
          <span style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>
            Checked against items you can currently act on. Rules fire against every open item at tick time —
            including ones still in Triage or blocked — which this check does not include.
          </span>
        </>
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

/** The severity a fresh rule starts at is the core's answer, not this
 * file's: the phone's form reads the same const, so the two clients cannot
 * birth rules at different ranks of ADR-0014's ratchet. */
function emptyEditorState(): RuleEditorState {
  return { name: "", eventKind: null, conditions: [], severity: defaultSeverity(), tier: "normal", enabled: true };
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

/** Whether `a` and `b` agree on every field the editor can touch — the
 * reseed gate `RuleCard` checks on every render, `screens/bindings.ts`'s
 * `sameBindingValue` pattern ported to a rule row: a pull carrying another
 * device's edit (or this device's own write landing) must never leave a
 * stale draft sitting over it with Save enabled to push the old values
 * back, and that includes the operator's own toggle click. */
function sameRuleEditableFields(a: RuleDTO, b: RuleDTO): boolean {
  return (
    a.name === b.name &&
    a.eventKind === b.eventKind &&
    a.severity === b.severity &&
    a.tier === b.tier &&
    a.enabled === b.enabled &&
    JSON.stringify(a.conditions) === JSON.stringify(b.conditions)
  );
}

function RuleEditorForm({
  registry,
  state,
  items,
  onChange,
  onSave,
  saveLabel,
}: {
  registry: KindRegistryDTO;
  state: RuleEditorState;
  items: TaskItemDTO[];
  onChange: (next: RuleEditorState) => void;
  onSave: () => void;
  saveLabel: string;
}) {
  const fields = fieldsForKind(registry, state.eventKind);
  const severityOptions = withCurrentOption(registry.severities, state.severity);
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
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
          <Select
            label="Severity"
            value={state.severity}
            options={severityOptions.map((s) => ({ value: s, label: s }))}
            onChange={(e) => onChange({ ...state, severity: e.target.value })}
          />
          {isUnrankedSeverity(registry.severities, state.severity) ? (
            <Badge tone="warn" icon="info">
              {UNRANKED_SEVERITY_COPY}
            </Badge>
          ) : null}
        </div>
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
      <BacktestPanel rule={state} items={items} />
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
  syncOutcomeSeq,
  lastRuleWrite,
  onToggle,
  onSave,
  onDelete,
}: {
  rule: RuleDTO;
  registry: KindRegistryDTO;
  items: TaskItemDTO[];
  syncOutcomeSeq: number;
  lastRuleWrite: TaskState["lastRuleWrite"];
  onToggle: (enabled: boolean) => void;
  onSave: (state: RuleEditorState) => void;
  onDelete: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<RuleEditorState>(() => editorStateFromRule(rule));
  // Two steps, because the write is not undoable from this screen: the
  // first click asks, the second sends. Not a `window.confirm` — that
  // blocks the worker's message pump and is untestable in jsdom.
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [pendingDelete, setPendingDelete] = useState(false);

  // Reseed the draft whenever the stored rule's own editable fields move
  // underneath it — another device's edit, or this device's own write
  // landing (which otherwise leaves the mount-time draft, including a
  // stale `enabled`, sitting over the new row with Save still enabled to
  // push it back). React's "adjust state while rendering" idiom, the same
  // one `SettingsScreen.tsx`'s `BindingRow` uses for the identical reason.
  const [seenRule, setSeenRule] = useState(rule);
  if (!sameRuleEditableFields(seenRule, rule)) {
    setSeenRule(rule);
    setDraft(editorStateFromRule(rule));
  }

  // The enable/disable switch is otherwise fully controlled by
  // `rule.enabled`: a click renders, then reverts the instant the DOM
  // reflects that same `rule.enabled` prop again, because the write is
  // only queued — up to a full cycle before it lands. Holding the clicked
  // value locally (and saying so) is what keeps the switch from lying
  // about what just happened; it clears the same render-time way, on the
  // next completed cycle or this rule's own write outcome, whichever
  // comes first.
  const [pendingEnabled, setPendingEnabled] = useState<boolean | null>(null);
  const [seenSyncOutcomeSeq, setSeenSyncOutcomeSeq] = useState(syncOutcomeSeq);
  const [seenLastRuleWrite, setSeenLastRuleWrite] = useState(lastRuleWrite);
  if (syncOutcomeSeq !== seenSyncOutcomeSeq || lastRuleWrite !== seenLastRuleWrite) {
    setSeenSyncOutcomeSeq(syncOutcomeSeq);
    setSeenLastRuleWrite(lastRuleWrite);
    if (pendingEnabled !== null) {
      setPendingEnabled(null);
    }
    // The queued delete clears the same render-time way and for the same
    // reason. A delete that landed takes the whole card with it (the
    // mirror stops listing the rule), so this only ever clears a delete
    // that has *not* landed — which is exactly when the badge should stop
    // claiming one is in flight.
    if (pendingDelete) {
      setPendingDelete(false);
    }
  }

  const valid = isRuleValid(rule, registry);
  const displayedEnabled = pendingEnabled ?? rule.enabled;
  const isPendingToggle = pendingEnabled !== null && pendingEnabled !== rule.enabled;

  return (
    <Card padding="var(--space-5)" style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
      <div className="hb-rule-card-header">
        <div className="hb-rule-card-title">
          <span style={{ font: "var(--type-body-strong)" }}>{rule.name}</span>
          <Badge tone={rule.tier === "urgent" ? "danger" : "info"} mono>
            {rule.tier}
          </Badge>
          <Badge tone="neutral" mono>
            {rule.eventKind === null ? "any kind" : kindLabel(rule.eventKind)}
          </Badge>
        </div>
        <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-end", gap: "var(--space-2)" }}>
          <Switch
            checked={displayedEnabled}
            onChange={(e) => {
              setPendingEnabled(e.target.checked);
              onToggle(e.target.checked);
            }}
            label="Enabled"
          />
          {isPendingToggle ? (
            <Badge dot mono tone="warn">
              pending
            </Badge>
          ) : null}
        </div>
      </div>
      {/* Sentence-length badges get their own wrapping row, below the
          identity row above — the pattern `ConditionRow`'s alarm-interval
          badge already uses. The identity row itself wraps name + mono
          status chips as a unit at narrow widths (#729); the sentence-length
          badges stay on their own row because they are prose-width, not
          fixed-width like the chips (#374). */}
      {!valid || isUnrankedSeverity(registry.severities, rule.severity) ? (
        <div style={{ display: "flex", flexWrap: "wrap", gap: "var(--space-2)" }}>
          {!valid ? (
            <Badge tone="danger" icon="info" wrap>
              Invalid — names a field its kind no longer declares
            </Badge>
          ) : null}
          {isUnrankedSeverity(registry.severities, rule.severity) ? (
            <Badge tone="warn" icon="info" wrap>
              {UNRANKED_SEVERITY_COPY}
            </Badge>
          ) : null}
        </div>
      ) : null}
      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-3)", flexWrap: "wrap" }}>
        <Button variant="ghost" size="sm" onClick={() => setEditing((v) => !v)}>
          {editing ? "Close" : "Edit"}
        </Button>
        {confirmingDelete ? (
          <>
            <span className="hb-meta">Delete rule?</span>
            <Button
              variant="danger"
              size="sm"
              onClick={() => {
                setConfirmingDelete(false);
                setPendingDelete(true);
                onDelete();
              }}
            >
              Delete
            </Button>
            <Button variant="ghost" size="sm" onClick={() => setConfirmingDelete(false)}>
              Cancel
            </Button>
          </>
        ) : (
          <Button variant="ghost" size="sm" onClick={() => setConfirmingDelete(true)}>
            Delete
          </Button>
        )}
        {pendingDelete ? (
          <Badge dot mono tone="warn">
            pending
          </Badge>
        ) : null}
      </div>
      {editing ? (
        <RuleEditorForm
          registry={registry}
          state={draft}
          items={items}
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
  syncOutcomeSeq: number;
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
      /** Present at all = touched, per `worker-client.ts`'s `patchRule`.
       * Deleting a rule is this field, not a call of its own. */
      deletedAt?: number | null;
    },
  ) => void;
}

export function RulesScreen({
  rules,
  kindRegistry,
  frontier,
  lastRuleWrite,
  syncOutcomeSeq,
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
            syncOutcomeSeq={syncOutcomeSeq}
            lastRuleWrite={lastRuleWrite}
            onToggle={(enabled) => onPatchRule(rule, { enabled })}
            onDelete={() => onPatchRule(rule, { deletedAt: Date.now() })}
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
            items={frontier}
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
