import type { ConditionDTO, RuleDTO, TaskItemDTO } from "../../store/protocol";
import type { OperatorName } from "./operators";
import { parseDurationMs } from "./duration";

// Backtest (ADR-0011): "re-fetch recent history from the source and show
// which events a draft rule *would have* promoted... needs no
// persistence... the match count is shown before save." This is a pure,
// client-side evaluation — never a server call, never a write — and it
// ports ADR-0013's evaluation semantics into TS deliberately (not a call
// into `hummingbird-rules-engine`, which is a native-only crate this build
// has no wasm path to) restricted to the one kind this client actually
// holds raw material for: `item_threshold`, synthesized from the mirrored
// item list the exact way `authority::sweep::item_threshold_event` does
// server-side, field for field. A rule targeting any other kind
// (`email`, `calendar_event`, `snapshot_change`, `alert_raised`, or "any
// kind") has no client-side event history to backtest against — this
// module says so explicitly rather than silently answering zero matches
// for a different reason.

export type BacktestUnavailableReason = "no_local_history";

export type BacktestResult =
  | { kind: "unavailable"; reason: BacktestUnavailableReason }
  | { kind: "ok"; matches: TaskItemDTO[] };

/** A day-only deadline resolves to 23:59 local (ADR-0009/0013's own rule);
 * anything else parses as a normal timestamp. `undefined` for anything
 * unparseable. */
function deadlineMs(value: string): number | undefined {
  const withTime = /^\d{4}-\d{2}-\d{2}$/.test(value) ? `${value}T23:59` : value;
  const ms = new Date(withTime).getTime();
  return Number.isNaN(ms) ? undefined : ms;
}

/** The `item_threshold` event fields this build can synthesize from one
 * mirrored item — the exact field set `authority::sweep::item_threshold_event`
 * populates, so a backtest match agrees with what the sweep would actually
 * evaluate. `project` is the raw `projectId`, never a resolved name — the
 * same value the server-side synthesis carries. */
function itemFields(item: TaskItemDTO): Record<string, string | number | boolean | undefined> {
  return {
    source: item.source ?? "",
    source_key: item.sourceKey ?? "",
    title: item.title,
    body: item.description ?? undefined,
    url: item.sourceUrl ?? undefined,
    deadline: item.deadline ?? undefined,
    scheduled_date: item.scheduledDate ?? undefined,
    stage: item.stage,
    size: item.size ?? undefined,
    energy: item.energy ?? undefined,
    context: item.context ?? undefined,
    priority: item.priority,
    project: item.projectId ?? undefined,
  };
}

function matchesOperator(op: OperatorName, fieldValue: unknown, conditionValue: unknown, nowMs: number): boolean {
  switch (op) {
    case "eq":
      return fieldValue === conditionValue;
    case "contains":
      if (Array.isArray(fieldValue)) {
        return fieldValue.includes(conditionValue);
      }
      return typeof fieldValue === "string" && typeof conditionValue === "string"
        ? fieldValue.includes(conditionValue)
        : false;
    case "gt":
      return typeof fieldValue === "number" && typeof conditionValue === "number" && fieldValue > conditionValue;
    case "lt":
      return typeof fieldValue === "number" && typeof conditionValue === "number" && fieldValue < conditionValue;
    case "is":
      return typeof fieldValue === "boolean" && fieldValue === conditionValue;
    case "within_next": {
      if (typeof fieldValue !== "string" || typeof conditionValue !== "string") {
        return false;
      }
      const t = deadlineMs(fieldValue);
      const durationMs = parseDurationMs(conditionValue);
      if (t === undefined || durationMs === undefined) {
        return false;
      }
      // "within_next D means t <= now + D — unbounded on the past side."
      return t <= nowMs + durationMs;
    }
    case "within_last": {
      if (typeof fieldValue !== "string" || typeof conditionValue !== "string") {
        return false;
      }
      const t = deadlineMs(fieldValue);
      const durationMs = parseDurationMs(conditionValue);
      if (t === undefined || durationMs === undefined) {
        return false;
      }
      return t >= nowMs - durationMs;
    }
  }
}

/** One condition against one item's synthesized fields. A missing field
 * makes the condition false, even negated (ADR-0013's evaluation rule 2). */
function matchesCondition(condition: ConditionDTO, fields: Record<string, unknown>, nowMs: number): boolean {
  const fieldValue = fields[condition.field];
  if (fieldValue === undefined) {
    return false;
  }
  const matched = matchesOperator(condition.op as OperatorName, fieldValue, condition.value, nowMs);
  return condition.negate ? !matched : matched;
}

/** Runs `rule`'s conditions (ANDed) against every mirrored item, as an
 * `item_threshold` event — the one kind this client can backtest with
 * fidelity to what the sweep actually evaluates. Any other `eventKind`
 * (including "any kind", which item_threshold still satisfies core-field
 * conditions for) reports `"unavailable"` rather than a misleading zero.
 *
 * Deliberately writes nothing and calls nothing — the whole point of
 * ADR-0011's "needs no persistence." */
export function backtest(rule: Pick<RuleDTO, "eventKind" | "conditions">, items: TaskItemDTO[], nowMs: number): BacktestResult {
  if (rule.eventKind !== null && rule.eventKind !== "item_threshold") {
    return { kind: "unavailable", reason: "no_local_history" };
  }
  const matches = items.filter((item) => {
    const fields = itemFields(item);
    return rule.conditions.every((condition) => matchesCondition(condition, fields, nowMs));
  });
  return { kind: "ok", matches };
}
