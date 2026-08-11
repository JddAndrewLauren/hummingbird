import type {
  BindingDTO,
  BindingValueDTO,
  BlockedFrontierEntryDTO,
  ConditionDTO,
  DeadLetterEntryDTO,
  FieldTypeName,
  FreshnessDTO,
  KindEntryDTO,
  KindRegistryDTO,
  PaneEnvelopeDTO,
  PaneReadDTO,
  ProjectDTO,
  RuleDTO,
  StepDTO,
  TaskEventDTO,
  TaskItemDTO,
  TaskRunOutcomeKind,
  TaskWorkerRequest,
  TaskWorkerResponse,
  TierName,
} from "../store/protocol";
import { createSerialQueue } from "./serial-queue";

// The worker's half of #105/S7's task binding, kept free of the wasm import
// so vitest (node) can exercise it against a fake `TaskHostLike` — same
// discipline as `calendar-worker.ts`/`announce.ts`.

/** The slice of `hummingbird-ffi-web`'s `TaskHost` this handler needs. Every
 * method mirrors the wasm-bindgen surface exactly (see
 * `client/ffi-web/src/lib.rs`'s `wasm_bindings::TaskHost`); the JSON
 * methods resolve to the string the core serializes, not a parsed value —
 * parsing is this module's job, so the wire format is explicit in one
 * place. */
export interface TaskHostLike {
  pushApiKey(apiKey: string): void;
  /** Issue #196 (shape 2)'s rehydration counterpart to `pushApiKey` — see
   * `protocol.ts`'s `initTaskApiKey` doc for the full contract. */
  rehydrateApiKey(apiKey: string): void;
  clearApiKey(): void;
  /** #208's `size`/`energy`/`context` are each `string | null` — the wire's
   * snake_case vocabulary names, resolved by name through
   * `hummingbird_domain::Size`/`Energy::parse` on the way in (never a raw
   * index), and `context` carried straight through. `null` is "not set". */
  capture(
    seed: string,
    title: string,
    stage: string,
    size: string | null,
    energy: string | null,
    context: string | null,
    nowMs: number,
  ): Promise<string>;
  act(seed: string, itemId: string, action: string, nowMs: number): Promise<string>;
  /** S13/#111's triage mutation. Mirrors `hummingbird-ffi-web`'s
   * `TaskHost::triage` exactly (`client/ffi-web/src/lib.rs`) — five
   * `undefined`-or-`string` edit fields plus `destination`, resolved to
   * JSON: `{"kind": "ok"|"not_found"|"failed"|"busy", "error": string|null}`. */
  triage(
    seed: string,
    itemId: string,
    destination: string,
    title: string | null,
    projectId: string | null,
    size: string | null,
    energy: string | null,
    context: string | null,
    nowMs: number,
  ): Promise<string>;
  /** #118's binding write. Mirrors `hummingbird-ffi-web`'s
   * `TaskHost::setBinding`, resolved to JSON:
   * `{"kind": "ok"|"unknown_key"|"failed"|"busy", "error": string|null}`. */
  setBinding(seed: string, key: string, value: string, nowMs: number): Promise<string>;
  bindings(): string;
  /** #140's kind registry export. Mirrors `hummingbird-ffi-web`'s
   * `TaskHost::kindRegistry` — never `"busy"`, so this resolves
   * synchronously to `{"kind":"ok",…}` always. */
  kindRegistry(): string;
  /** #140's rules read. Mirrors `TaskHost::rules`, resolved to JSON:
   * `{"kind": "ok"|"busy", "rules": [Rule]}`. */
  rules(): string;
  /** #140's rule create. Mirrors `TaskHost::createRule`, resolved to JSON:
   * `{"kind": "ok"|"failed"|"busy", "id": string|null, "error": string|null}`.
   * `conditionsJson` is `Condition[]`'s own JSON array. */
  createRule(
    seed: string,
    name: string,
    eventKind: string | null,
    conditionsJson: string,
    severity: string,
    tier: string,
    enabled: boolean,
    nowMs: number,
  ): Promise<string>;
  /** #140's rule patch — the enable/disable toggle and every other rule
   * edit. Mirrors `TaskHost::patchRule`, resolved to JSON:
   * `{"kind": "ok"|"failed"|"busy", "error": string|null}`. `currentJson`
   * is the caller's own last-known `Rule`, as JSON — the CAS `base` a 409
   * is diffed against. `conditionsJson` is `Condition[]`'s own JSON array,
   * or `undefined` to leave conditions untouched. */
  patchRule(
    seed: string,
    currentJson: string,
    name: string | null,
    eventKindTouched: boolean,
    eventKind: string | null,
    conditionsJson: string | null,
    severity: string | null,
    tier: string | null,
    enabled: boolean | null,
    nowMs: number,
  ): Promise<string>;
  /** #245's generic pane read. Mirrors `hummingbird-ffi-web`'s
   * `TaskHost::paneRead` — see `RawPaneReadResponse` below for the wire
   * shape, which `client/ffi-web/src/task_host.rs` pins byte-for-byte. */
  paneRead(source: string, nowMs: number): string;
  frontier(): string;
  triageInbox(): string;
  blocked(): string;
  steps(itemId: string): string;
  projects(): string;
  isPending(itemId: string): string;
  takeEvents(): string;
  runSync(
    nowMs: number,
    trigger: string,
    forceFullSweep: boolean,
    jitterUnit: number,
  ): Promise<string>;
  queueDepth(): string;
  deadLetters(): string;
  mirrorSnapshot(): string;
}

interface RawItem {
  id: string;
  seq: number | null;
  title: string;
  description: string | null;
  stage: string;
  size: string | null;
  energy: string | null;
  context: string | null;
  priority: number;
  project_id: string | null;
  project_pos: number | null;
  deadline: string | null;
  scheduled_date: string | null;
  source: string | null;
  source_key: string | null;
  source_url: string | null;
  archived_at: number | null;
  created_at: number;
  updated_at: number;
  version: number;
  /** Flattened alongside the item's own fields (`FrontierItemDTO`,
   * `client/ffi-web/src/task_host.rs`) — issue #108's "a pending item is
   * marked as such". */
  pending: boolean;
}

/** The core's own `BindingValue` serde shape — already camel-free (its keys
 * are `state`/`text`/`raw`), so it crosses unchanged. */
type RawBindingValue = BindingValueDTO;

interface RawBinding {
  key: string;
  known: boolean;
  pending: boolean;
  value: RawBindingValue;
}

interface RawBindingListResponse {
  kind: "ok" | "busy";
  bindings: RawBinding[];
}

interface RawSetBindingResponse {
  kind: "ok" | "unknown_key" | "failed" | "busy";
  error: string | null;
}

// -- rules (#140) ------------------------------------------------------

interface RawCondition {
  field: string;
  op: string;
  value: unknown;
  negate: boolean;
}

interface RawRule {
  id: string;
  name: string;
  event_kind: string | null;
  conditions: RawCondition[];
  severity: string;
  tier: TierName;
  enabled: boolean;
  updated_at: number;
  version: number;
}

interface RawRuleListResponse {
  kind: "ok" | "busy";
  rules: RawRule[];
}

interface RawCreateRuleResponse {
  kind: "ok" | "failed" | "busy";
  id: string | null;
  error: string | null;
}

interface RawPatchRuleResponse {
  kind: "ok" | "failed" | "busy";
  error: string | null;
}

interface RawKindField {
  name: string;
  field_type: FieldTypeName;
}

interface RawKindEntry {
  key: string;
  mints: boolean;
  fields: RawKindField[];
}

interface RawKindRegistryResponse {
  kind: "ok";
  kinds: RawKindEntry[];
  core_fields: RawKindField[];
  alarm_interval_ms: number;
  severities: string[];
}

// -- the pane read (#245) — pinned to `PaneReadResponse`'s serde output by
// `client/ffi-web/src/task_host.rs`'s
// `pane_read_response_serializes_with_the_exact_keys_the_pane_shell_ts_parses`.

type RawFreshness =
  | { state: "unknown" }
  | { state: "age"; age_ms: number; declared_cadence_ms: number | null };

type RawPaneEnvelope =
  | { state: "parsed"; schema: string; polled_every_ms: number | null; body: string }
  | { state: "malformed"; reason: string };

interface RawPaneSnapshot {
  source: string;
  key: string;
  fetched_at: number;
  version: number;
  freshness: RawFreshness;
  envelope: RawPaneEnvelope;
}

/** A raw `hummingbird_domain::Alert` row. Only the fields a pane reads are
 * named — the rest ride along in the JSON and are dropped by `mapPaneRead`,
 * deliberately: `AlertsScreen` is the surface for the whole row, and a pane
 * that could reach `severity` or `source_key` would be one join away from
 * re-deriving occurrence identity, which ADR-0015 says is never a join key. */
interface RawPaneAlert {
  id: string;
  subject_key: string | null;
  title: string;
  body: string | null;
  raised_at: number;
  expires_at: number | null;
}

interface RawPaneReadResponse {
  kind: "ok" | "busy";
  snapshots: RawPaneSnapshot[];
  alerts: RawPaneAlert[];
}

interface RawProject {
  id: string;
  name: string;
  archived_at: number | null;
  created_at: number;
  updated_at: number;
  version: number;
}

interface RawProjectListResponse {
  kind: "ok" | "busy";
  projects: RawProject[];
}

interface RawItemListResponse {
  kind: "ok" | "busy";
  items: RawItem[];
}

interface RawBlockedEntry {
  item: RawItem;
  blocked_by: RawItem[];
}

interface RawBlockedListResponse {
  kind: "ok" | "busy";
  entries: RawBlockedEntry[];
}

interface RawStep {
  id: string;
  item_id: string;
  body: string;
  done: boolean;
  position: number;
  deleted_at: number | null;
  version: number;
}

interface RawStepListResponse {
  kind: "ok" | "busy";
  steps: RawStep[];
}

interface RawIsPendingResponse {
  kind: "ok" | "busy";
  pending: boolean;
}

interface RawCaptureResponse {
  kind: "ok" | "failed" | "busy";
  id: string | null;
  error: string | null;
}

interface RawActResponse {
  kind: "ok" | "not_found" | "failed" | "busy";
  error: string | null;
}

interface RawTriageResponse {
  kind: "ok" | "not_found" | "failed" | "busy";
  error: string | null;
}

interface RawTaskEvent {
  kind: "credential_needed";
  at_ms: number;
}

interface RawRunResponse {
  kind: TaskRunOutcomeKind;
  retry_after_ms: number | null;
  active_item_count: number | null;
  // `Option<bool>` on the Rust side (`RunResponse::was_full_sweep`,
  // `client/ffi-web/src/task_host.rs`) serializes to a JSON boolean or
  // `null`, never a number — pinned by that file's own
  // `run_response_serializes_with_the_exact_keys_and_kind_literals_task_worker_ts_parses`
  // test, so this is the real wire shape, not a defensive guess at it.
  was_full_sweep: boolean | null;
  dead_lettered: number | null;
}

interface RawQueueDepthResponse {
  kind: "ok" | "busy";
  depth: number;
}

interface RawDeadLetterField {
  field: string;
  local: unknown;
  server: unknown;
}

interface RawDeadLetterEntry {
  id: string;
  // Kept in step with `protocol.ts`'s `DeadLetterEntryDTO["reason"]`;
  // `"contention"` is #163's third variant, carrying neither `message` nor
  // any `fields`.
  reason: "permanent" | "conflict" | "contention";
  message: string | null;
  fields: RawDeadLetterField[];
  at_ms: number;
}

interface RawDeadLettersResponse {
  kind: "ok" | "busy";
  entries: RawDeadLetterEntry[];
}

interface RawMirrorSnapshotResponse {
  kind: "ok" | "busy";
  mirror: unknown;
}

function mapItem(raw: RawItem): TaskItemDTO {
  return {
    id: raw.id,
    seq: raw.seq,
    title: raw.title,
    description: raw.description,
    stage: raw.stage as TaskItemDTO["stage"],
    size: raw.size as TaskItemDTO["size"],
    energy: raw.energy as TaskItemDTO["energy"],
    context: raw.context,
    priority: raw.priority,
    projectId: raw.project_id,
    projectPos: raw.project_pos,
    deadline: raw.deadline,
    scheduledDate: raw.scheduled_date,
    source: raw.source,
    sourceKey: raw.source_key,
    sourceUrl: raw.source_url,
    archivedAt: raw.archived_at,
    createdAt: raw.created_at,
    updatedAt: raw.updated_at,
    version: raw.version,
    pending: raw.pending,
  };
}

function mapBinding(raw: RawBinding): BindingDTO {
  return {
    key: raw.key,
    known: raw.known,
    pending: raw.pending,
    value: raw.value,
  };
}

function mapCondition(raw: RawCondition): ConditionDTO {
  return { field: raw.field, op: raw.op, value: raw.value, negate: raw.negate };
}

function mapRule(raw: RawRule): RuleDTO {
  return {
    id: raw.id,
    name: raw.name,
    eventKind: raw.event_kind,
    conditions: raw.conditions.map(mapCondition),
    severity: raw.severity,
    tier: raw.tier,
    enabled: raw.enabled,
    updatedAt: raw.updated_at,
    version: raw.version,
  };
}

function mapKindField(raw: RawKindField): KindEntryDTO["fields"][number] {
  return { name: raw.name, fieldType: raw.field_type };
}

function mapKindEntry(raw: RawKindEntry): KindEntryDTO {
  return { key: raw.key, mints: raw.mints, fields: raw.fields.map(mapKindField) };
}

function mapKindRegistry(raw: RawKindRegistryResponse): KindRegistryDTO {
  return {
    kinds: raw.kinds.map(mapKindEntry),
    coreFields: raw.core_fields.map(mapKindField),
    alarmIntervalMs: raw.alarm_interval_ms,
    severities: raw.severities,
  };
}

function mapFreshness(raw: RawFreshness): FreshnessDTO {
  return raw.state === "unknown"
    ? { kind: "unknown" }
    : { kind: "age", ageMs: raw.age_ms, declaredCadenceMs: raw.declared_cadence_ms };
}

function mapEnvelope(raw: RawPaneEnvelope): PaneEnvelopeDTO {
  return raw.state === "parsed"
    ? { kind: "ok", schema: raw.schema, polledEveryMs: raw.polled_every_ms, body: raw.body }
    : { kind: "malformed", reason: raw.reason };
}

function mapPaneRead(source: string, raw: RawPaneReadResponse): PaneReadDTO {
  return {
    source,
    snapshots: raw.snapshots.map((snapshot) => ({
      key: snapshot.key,
      fetchedAtMs: snapshot.fetched_at,
      envelope: mapEnvelope(snapshot.envelope),
      freshness: mapFreshness(snapshot.freshness),
    })),
    liveAlerts: raw.alerts.map((alert) => ({
      id: alert.id,
      subjectKey: alert.subject_key,
      title: alert.title,
      body: alert.body,
      raisedAtMs: alert.raised_at,
      expiresAtMs: alert.expires_at,
    })),
  };
}

function mapProject(raw: RawProject): ProjectDTO {
  return {
    id: raw.id,
    name: raw.name,
    archivedAt: raw.archived_at,
    createdAt: raw.created_at,
    updatedAt: raw.updated_at,
    version: raw.version,
  };
}

function mapBlockedEntries(raw: RawBlockedEntry[]): BlockedFrontierEntryDTO[] {
  return raw.map((entry) => ({
    item: mapItem(entry.item),
    blockedBy: entry.blocked_by.map(mapItem),
  }));
}

function mapStep(raw: RawStep): StepDTO {
  return {
    id: raw.id,
    itemId: raw.item_id,
    body: raw.body,
    done: raw.done,
    position: raw.position,
    deletedAt: raw.deleted_at,
    version: raw.version,
  };
}

function mapEvents(raw: RawTaskEvent[]): TaskEventDTO[] {
  return raw.map((event) => ({ kind: event.kind, atMs: event.at_ms }));
}

function mapDeadLetters(raw: RawDeadLetterEntry[]): DeadLetterEntryDTO[] {
  return raw.map((entry) => ({
    id: entry.id,
    reason: entry.reason,
    message: entry.message,
    fields: entry.fields.map((field) => ({
      field: field.field,
      local: field.local,
      server: field.server,
    })),
    atMs: entry.at_ms,
  }));
}

/** Drains and broadcasts task events, if any. Called after every request
 * that could have produced one (today, only a `runSync` that holds on a
 * rejected credential does).
 *
 * `post` here is `PortRegistry.broadcast`, not a reply to whichever port
 * triggered the drain — deliberately: `Core::take_events` is a destructive,
 * single-reader drain (#104's own review finding), so posting only back to
 * the requesting port would let the first tab to poll silently swallow an
 * event every other connected tab needed too (#105's second load-bearing
 * handoff). Fanning it out here, at the one place every task request's
 * response already flows through, is what closes that gap without adding a
 * second broadcast path. */
function postTaskEvents(host: TaskHostLike, post: (response: TaskWorkerResponse) => void): void {
  const raw = JSON.parse(host.takeEvents()) as RawTaskEvent[];
  if (raw.length > 0) {
    post({ type: "taskEvents", events: mapEvents(raw) });
  }
}

/** Reads and posts the outbound queue's current depth, unless the host
 * reports itself busy — "no answer, not an empty answer" (same contract as
 * every other `"busy"` read in this module). Shared between the
 * request-driven `getQueueDepth` case and the `runSync` tail push (issue
 * #191) so there is exactly one place this response is built, and the tail
 * push can never drift from what an explicit request would have produced. */
function postQueueDepth(host: TaskHostLike, post: (response: TaskWorkerResponse) => void): void {
  const raw = JSON.parse(host.queueDepth()) as RawQueueDepthResponse;
  if (raw.kind === "busy") {
    return;
  }
  post({ type: "queueDepth", depth: raw.depth });
}

/** Reads and posts the whole dead-letter journal, unless the host reports
 * itself busy — same contract as [`postQueueDepth`]. Shared between
 * `getDeadLetters` and the `runSync` tail push (issue #191): called at most
 * once per cycle regardless of how many connected views end up seeing the
 * broadcast, which is what keeps the journal's serialization from scaling
 * with view count the way the per-view refresh this replaces did. */
function postDeadLetters(host: TaskHostLike, post: (response: TaskWorkerResponse) => void): void {
  const raw = JSON.parse(host.deadLetters()) as RawDeadLettersResponse;
  if (raw.kind === "busy") {
    return;
  }
  post({ type: "deadLetters", entries: mapDeadLetters(raw.entries) });
}

/** Handles one `TaskWorkerRequest`, posting whatever `TaskWorkerResponse`(s)
 * it produces. Callers should go through [`createTaskRequestQueue`] rather
 * than calling this directly — see that function's own doc for why. */
export async function handleTaskRequest(
  request: TaskWorkerRequest,
  host: TaskHostLike,
  post: (response: TaskWorkerResponse) => void,
): Promise<void> {
  switch (request.type) {
    case "pushTaskApiKey":
      // Forwarded and never echoed: no branch below, or anywhere else in
      // this module, ever posts a message carrying an API key.
      host.pushApiKey(request.apiKey);
      return;
    case "initTaskApiKey":
      // Issue #196 (shape 2): rehydration, never rotation — forwarded to
      // the host method that never resumes a hold. Also never echoed.
      host.rehydrateApiKey(request.apiKey);
      return;
    case "clearTaskApiKey":
      // "Forget token" (#106/S8): forwarded and never acknowledged with a
      // reply — there is nothing to reply with, and the host already
      // updated its own local state before sending this.
      host.clearApiKey();
      return;
    case "capture": {
      const raw = JSON.parse(
        await host.capture(
          request.seed,
          request.title,
          request.stage,
          request.size,
          request.energy,
          request.context,
          request.nowMs,
        ),
      ) as RawCaptureResponse;
      post({ type: "captureResult", seed: request.seed, kind: raw.kind, id: raw.id, error: raw.error });
      postTaskEvents(host, post);
      return;
    }
    case "act": {
      const raw = JSON.parse(
        await host.act(request.seed, request.itemId, request.action, request.nowMs),
      ) as RawActResponse;
      post({
        type: "actResult",
        seed: request.seed,
        itemId: request.itemId,
        action: request.action,
        kind: raw.kind,
        error: raw.error,
      });
      return;
    }
    case "triage": {
      const raw = JSON.parse(
        await host.triage(
          request.seed,
          request.itemId,
          request.destination,
          request.title,
          request.projectId,
          request.size,
          request.energy,
          request.context,
          request.nowMs,
        ),
      ) as RawTriageResponse;
      post({
        type: "triageResult",
        seed: request.seed,
        itemId: request.itemId,
        kind: raw.kind,
        error: raw.error,
      });
      return;
    }
    case "setBinding": {
      const raw = JSON.parse(
        await host.setBinding(request.seed, request.key, request.value, request.nowMs),
      ) as RawSetBindingResponse;
      post({
        type: "setBindingResult",
        seed: request.seed,
        key: request.key,
        kind: raw.kind,
        error: raw.error,
      });
      return;
    }
    case "getBindings": {
      const raw = JSON.parse(host.bindings()) as RawBindingListResponse;
      if (raw.kind === "busy") {
        // No answer, not an empty answer — an empty binding list reads as
        // "nothing is bound", which is the wrong answer rather than none.
        return;
      }
      post({ type: "bindings", bindings: raw.bindings.map(mapBinding) });
      return;
    }
    case "getKindRegistry": {
      const raw = JSON.parse(host.kindRegistry()) as RawKindRegistryResponse;
      post({ type: "kindRegistry", registry: mapKindRegistry(raw) });
      return;
    }
    case "getRules": {
      const raw = JSON.parse(host.rules()) as RawRuleListResponse;
      if (raw.kind === "busy") {
        // No answer, not an empty one — an empty rule list reads as "no
        // rules exist," same contract as `getBindings`.
        return;
      }
      post({ type: "rules", rules: raw.rules.map(mapRule) });
      return;
    }
    case "createRule": {
      const raw = JSON.parse(
        await host.createRule(
          request.seed,
          request.name,
          request.eventKind,
          JSON.stringify(request.conditions),
          request.severity,
          request.tier,
          request.enabled,
          request.nowMs,
        ),
      ) as RawCreateRuleResponse;
      post({
        type: "createRuleResult",
        seed: request.seed,
        kind: raw.kind,
        id: raw.id,
        error: raw.error,
      });
      return;
    }
    case "patchRule": {
      const raw = JSON.parse(
        await host.patchRule(
          request.seed,
          JSON.stringify({
            id: request.current.id,
            name: request.current.name,
            event_kind: request.current.eventKind,
            conditions: request.current.conditions.map((c) => ({
              field: c.field,
              op: c.op,
              value: c.value,
              negate: c.negate,
            })),
            severity: request.current.severity,
            tier: request.current.tier,
            enabled: request.current.enabled,
            updated_at: request.current.updatedAt,
            version: request.current.version,
          }),
          request.name,
          request.eventKindTouched,
          request.eventKind,
          request.conditions === null ? null : JSON.stringify(request.conditions),
          request.severity,
          request.tier,
          request.enabled,
          request.nowMs,
        ),
      ) as RawPatchRuleResponse;
      post({
        type: "patchRuleResult",
        seed: request.seed,
        ruleId: request.current.id,
        kind: raw.kind,
        error: raw.error,
      });
      return;
    }
    case "getPaneRead": {
      const raw = JSON.parse(host.paneRead(request.source, request.nowMs)) as RawPaneReadResponse;
      if (raw.kind === "busy") {
        // No answer, not an empty answer — and it matters more here than
        // most: an empty pane read renders as "nothing is due tonight",
        // which is a claim, not a blank (`lib.rs`'s `BUSY_PANE_READ`).
        return;
      }
      post({ type: "paneRead", read: mapPaneRead(request.source, raw) });
      return;
    }
    case "getFrontier": {
      const raw = JSON.parse(host.frontier()) as RawItemListResponse;
      if (raw.kind === "busy") {
        // No answer, not an empty answer — busy says nothing about the
        // frontier's real contents (same contract as the calendar tile's
        // "busy", protocol.ts).
        return;
      }
      post({ type: "frontier", items: raw.items.map(mapItem) });
      return;
    }
    case "getTriageInbox": {
      const raw = JSON.parse(host.triageInbox()) as RawItemListResponse;
      if (raw.kind === "busy") {
        return;
      }
      post({ type: "triageInbox", items: raw.items.map(mapItem) });
      return;
    }
    case "getBlocked": {
      const raw = JSON.parse(host.blocked()) as RawBlockedListResponse;
      if (raw.kind === "busy") {
        return;
      }
      post({ type: "blocked", entries: mapBlockedEntries(raw.entries) });
      return;
    }
    case "getSteps": {
      const raw = JSON.parse(host.steps(request.itemId)) as RawStepListResponse;
      if (raw.kind === "busy") {
        return;
      }
      post({ type: "steps", itemId: request.itemId, steps: raw.steps.map(mapStep) });
      return;
    }
    case "getProjects": {
      const raw = JSON.parse(host.projects()) as RawProjectListResponse;
      if (raw.kind === "busy") {
        return;
      }
      post({ type: "projects", projects: raw.projects.map(mapProject) });
      return;
    }
    case "isPending": {
      const raw = JSON.parse(host.isPending(request.itemId)) as RawIsPendingResponse;
      if (raw.kind === "busy") {
        return;
      }
      post({ type: "isPendingResult", itemId: request.itemId, pending: raw.pending });
      return;
    }
    case "runSync": {
      const raw = JSON.parse(
        await host.runSync(request.nowMs, request.trigger, request.forceFullSweep, request.jitterUnit),
      ) as RawRunResponse;
      post({
        type: "syncOutcome",
        kind: raw.kind,
        retryAfterMs: raw.retry_after_ms,
        activeItemCount: raw.active_item_count,
        wasFullSweep: raw.was_full_sweep,
        deadLettered: raw.dead_lettered,
        // Issue #195 round-1 review: the cycle's OWN time, not whatever
        // clock happens to be running when a view later receives this (live
        // or replayed via `PortRegistry`) — see protocol.ts's `syncOutcome`
        // doc.
        atMs: request.nowMs,
      });
      postTaskEvents(host, post);
      // Issue #191: pushed unsolicited at the tail of every cycle, so N
      // connected views cost one `queueDepth()`/`deadLetters()` wasm call
      // pair per cycle — not one pair per view, per the triage ruling's
      // "unsolicited push" shape. Each is still broadcast to every port
      // (`PortRegistry.broadcast`), so the message count still scales with
      // view count — only the wasm-call count and the journal
      // serialization are now constant per cycle. See protocol.ts's
      // `getQueueDepth`/`getDeadLetters`/`queueDepth`/`deadLetters` docs.
      postQueueDepth(host, post);
      postDeadLetters(host, post);
      return;
    }
    case "getQueueDepth":
      postQueueDepth(host, post);
      return;
    case "getDeadLetters":
      postDeadLetters(host, post);
      return;
    case "getMirrorSnapshot": {
      const raw = JSON.parse(host.mirrorSnapshot()) as RawMirrorSnapshotResponse;
      if (raw.kind === "busy") {
        return;
      }
      post({ type: "mirrorSnapshot", mirror: raw.mirror });
      return;
    }
  }
}

/** How long one task request may run before the queue abandons it rather
 * than stalling every request behind it (issue #95's named risk: "a hung
 * request wedges the worker"). Generous relative to a normal sync cycle
 * (a handful of HTTP round trips) so a slow-but-completing cycle is never
 * mistaken for a hang; see `serial-queue.ts` for what "abandoned" means and
 * why it is safe. */
export const TASK_REQUEST_TIMEOUT_MS = 30_000;

/** Serialises every task request into one at-a-time chain — its own queue,
 * independent of the calendar binding's (`calendar-worker.ts`'s
 * `createRequestQueue`): the two wrap different Rust objects with their own
 * independent check-out/check-in re-entrancy guards, so nothing requires
 * ordering a `capture` against a calendar poll. Built on `serial-queue.ts`'s
 * generic abandon-on-timeout queue rather than a second copy of the
 * ordering logic `createRequestQueue` already has. */
export function createTaskRequestQueue(
  host: TaskHostLike,
  post: (response: TaskWorkerResponse) => void,
): (request: TaskWorkerRequest) => Promise<void> {
  return createSerialQueue(
    (request: TaskWorkerRequest) => handleTaskRequest(request, host, post),
    {
      timeoutMs: TASK_REQUEST_TIMEOUT_MS,
      onTimeout: (request) => {
        console.error("task worker request abandoned after timeout", request.type);
      },
      onError: (request, error) => {
        console.error("task worker request failed", request.type, error);
      },
    },
  );
}
