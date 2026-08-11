import type { ConditionDTO, RuleDTO, TaskItemDTO } from "../../store/protocol";
import type { OperatorName } from "./operators";
import { parseDurationMs } from "./duration";

// Backtest (ADR-0011): "re-fetch recent history from the source and show
// which events a draft rule *would have* promoted... needs no
// persistence... the match count is shown before save." This is a pure,
// client-side evaluation — never a server call, never a write — and it
// ports ADR-0013's evaluation semantics into TS deliberately (not a call
// into `hummingbird-rules-engine`, which is a native-only crate this build
// has no wasm path to), restricted to the one kind this client actually
// holds raw material for: `item_threshold`. A rule targeting any other kind
// (`email`, `calendar_event`, `snapshot_change`, `alert_raised`) has no
// client-side event history to backtest against and reports
// `"unavailable"` rather than silently answering zero matches for a
// different reason. `eventKind === null` ("any kind") is still evaluated —
// `item_threshold_event` synthesizes it just like every other source, and a
// rule matching "any kind" matches this one too — so it is `"ok"`, not
// `"unavailable"`, the same reading `sweep_tick` itself gives a null
// `event_kind`.
//
// **Two honest gaps against `authority::sweep::item_threshold_event`,
// neither hidden by the count this reports:**
//
// 1. **The corpus.** `sweep_tick` evaluates every non-archived item
//    (`load_live_items`); this backtest only ever sees whatever `items`
//    the caller passes it, which today is `task.frontier` —
//    `Ready`/`InProgress`, unarchived, *and* unblocked (`Core::frontier`).
//    Triage-stage and blocked items are outside the count. The UI copy
//    names this explicitly rather than presenting a bare "N matches" a
//    reader could mistake for the sweep's own answer.
// 2. **`itemFields` cannot synthesize every core field with full
//    fidelity.** `occurred_at` is derived here from `item.updatedAt` the
//    same way the server derives it from `item.updated_at`
//    (`now_as_deadline`), and `calendar_busy` is always `false` here
//    because the server's own synthesis always writes `None` for it
//    (`item_threshold_event` never sets `calendar_busy` — `Event::core_value`
//    is what resolves that to `false`), so both are exact, not
//    approximate. Every other field this module offers is a direct 1:1
//    copy of what `item_threshold_event` sets.

export type BacktestUnavailableReason = "no_local_history";

export type BacktestResult =
  | { kind: "unavailable"; reason: BacktestUnavailableReason }
  | { kind: "ok"; matches: TaskItemDTO[] };

/** A day-only deadline resolves to 23:59 local (ADR-0009/0013's own rule);
 * anything else parses as a normal timestamp, local time — a bare
 * `YYYY-MM-DDTHH:MM` with no zone suffix is exactly what the JS `Date`
 * constructor treats as local, which agrees with `deadline`/`scheduled_date`'s
 * own reading elsewhere in this app (`urgency.ts`, `deadlineSortKey`).
 * `undefined` for anything unparseable. */
function deadlineMs(value: string): number | undefined {
  const withTime = /^\d{4}-\d{2}-\d{2}$/.test(value) ? `${value}T23:59` : value;
  const ms = new Date(withTime).getTime();
  return Number.isNaN(ms) ? undefined : ms;
}

/** `occurred_at`'s own parse — unlike `deadline`/`scheduled_date`, its wire
 * value is UTC (`now_as_deadline`'s own doc: "the Worker runs UTC. Not
 * per-rule, not per-device"), so it must not go through the bare `Date`
 * constructor's local-time reading `deadlineMs` uses. Appending `Z` is what
 * forces UTC interpretation of the same `YYYY-MM-DDTHH:MM` shape. */
function occurredAtMs(value: string): number | undefined {
  const ms = new Date(`${value}Z`).getTime();
  return Number.isNaN(ms) ? undefined : ms;
}

/** The TS twin of `hummingbird_domain::deadline::now_as_deadline` —
 * minute-precision, UTC, seconds truncated never rounded. This is what
 * lets `itemFields`' `occurred_at` be an exact match for the server's own
 * `item_threshold_event(item).occurred_at` (`now_as_deadline(item.updated_at)`),
 * not merely a plausible stand-in. */
function nowAsDeadline(ms: number): string {
  const d = new Date(Math.floor(ms / 60_000) * 60_000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}T${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`;
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
    // `item_threshold_event` sets `occurred_at: now_as_deadline(item.updated_at)`
    // — a poll-time-derived core field, not a stored one, but derivable
    // exactly from the same `updated_at` this DTO already carries.
    occurred_at: nowAsDeadline(item.updatedAt),
    title: item.title,
    body: item.description ?? undefined,
    url: item.sourceUrl ?? undefined,
    // The server's synthesis never sets `calendar_busy` at all
    // (`item_threshold_event` leaves it `None`); `Event::core_value`
    // resolves that to `false`, ADR-0013's one "missing is false" field, so
    // matching it exactly means always answering `false` here too.
    calendar_busy: false,
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

function matchesOperator(
  field: string,
  op: OperatorName,
  fieldValue: unknown,
  conditionValue: unknown,
  nowMs: number,
): boolean {
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
      const t = field === "occurred_at" ? occurredAtMs(fieldValue) : deadlineMs(fieldValue);
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
      const t = field === "occurred_at" ? occurredAtMs(fieldValue) : deadlineMs(fieldValue);
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
  const matched = matchesOperator(condition.field, condition.op as OperatorName, fieldValue, condition.value, nowMs);
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
