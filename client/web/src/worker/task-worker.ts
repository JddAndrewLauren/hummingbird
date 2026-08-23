import type {
  BindingDTO,
  QuestionSwitchDTO,
  BindingValueDTO,
  BlockedFrontierEntryDTO,
  ConditionDTO,
  DeadLetterEntryDTO,
  FieldTypeName,
  GrillDraftTurnDTO,
  KindEntryDTO,
  KindRegistryDTO,
  LedgerRowDTO,
  PaneEnvelopeDTO,
  PaneReadDTO,
  ProjectDTO,
  ProjectLinkDTO,
  RecallGroup,
  RecallRowDTO,
  RouteDTO,
  RuleDTO,
  StepDTO,
  TaskEventDTO,
  TaskItemDTO,
  TaskRunOutcomeKind,
  TaskWorkerRequest,
  TaskWorkerResponse,
  TierName,
} from "../store/protocol";
import { mapFreshness, type RawFreshness } from "./freshness-wire";
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
  /** #208's capture fields as one JSON object (`ffi-web`'s `CaptureFields`),
   * for the same reason `triage` below takes one: the list had grown past the
   * point where positional scalars read. `size`/`energy` inside it are the
   * wire's snake_case vocabulary names, resolved by name through
   * `hummingbird_domain::Size`/`Energy::parse` on the way in (never a raw
   * index); `null` on any key is "not set". */
  capture(
    seed: string,
    title: string,
    stage: string,
    fields: string,
    nowMs: number,
  ): Promise<string>;
  act(seed: string, itemId: string, action: string, nowMs: number): Promise<string>;
  /** S13/#111's triage mutation. Mirrors `hummingbird-ffi-web`'s
   * `TaskHost::triage` exactly (`client/ffi-web/src/lib.rs`): `destination`
   * plus the edits as one JSON object — NOT a positional field list, because
   * only an object can carry the difference between a key being absent
   * ("leave this field alone") and `null` ("clear it"). Resolved to JSON:
   * `{"kind": "ok"|"not_found"|"failed"|"busy", "error": string|null}`, where
   * an unreadable `edits` payload is one of the `"failed"` answers.
   * `destination` is `null` (#122) to leave `stage` untouched entirely — the
   * weekend-plans pane's do-date chip triages an item that may already be
   * `InProgress`, which the destination vocabulary cannot name. */
  triage(
    seed: string,
    itemId: string,
    destination: string | null,
    edits: string,
    nowMs: number,
  ): Promise<string>;
  /** #355/ADR-0023's Grill-completion mutation. Mirrors
   * `hummingbird-ffi-web`'s `TaskHost::completeGrill`, resolved to JSON:
   * `{"kind": "ok"|"not_found"|"item_done"|"needs_re_review"|"failed"|"busy", "id": string|null, "error": string|null}`.
   * `sessionStepsJson` is `Step[]`'s own JSON array, snake_case
   * (`unmapStep`) — `hummingbird_domain::Step` carries no `rename_all`.
   * `verdict` is the wire's snake_case spelling
   * (`"resolved"`/`"fog_remains"`). */
  completeGrill(
    seed: string,
    itemId: string,
    sessionStepsJson: string,
    transcript: string,
    summary: string,
    verdict: string,
    modelProposal: string,
    appliedPatch: string,
    deleteUntickedPlan: boolean,
    nowMs: number,
  ): Promise<string>;
  /** #356/ADR-0023's draft save. Mirrors `hummingbird-ffi-web`'s
   * `TaskHost::saveGrillDraft`, resolved to JSON:
   * `{"kind": "ok"|"failed"|"busy", "error": string|null}`. `turnsJson` is
   * the caller's own opaque `GrillTurn[]`, stringified whole — the core
   * never parses its shape, only stores and returns it verbatim. */
  saveGrillDraft(itemId: string, turnsJson: string, nowMs: number): Promise<string>;
  /** #356's explicit "Discard" gesture. Mirrors `TaskHost::discardGrillDraft`,
   * resolved to JSON: `{"kind": "ok"|"failed"|"busy", "error": string|null}`. */
  discardGrillDraft(itemId: string, nowMs: number): Promise<string>;
  /** #356's resume read. Mirrors `TaskHost::grillDraft`, resolved to JSON:
   * `{"kind": "ok"|"busy", "exists": bool, "turns": array|null}`. */
  grillDraft(itemId: string): string;
  /** #356's bulk read. Mirrors `TaskHost::grillDraftItemIds`, resolved to
   * JSON: `{"kind": "ok"|"busy", "item_ids": [string]}`. */
  grillDraftItemIds(): string;
  /** #118's binding write. Mirrors `hummingbird-ffi-web`'s
   * `TaskHost::setBinding`, resolved to JSON:
   * `{"kind": "ok"|"unknown_key"|"failed"|"busy", "error": string|null}`. */
  setBinding(seed: string, key: string, value: string, nowMs: number): Promise<string>;
  bindings(): string;
  /** #715's toggle write. Mirrors `hummingbird-ffi-web`'s
   * `TaskHost::setQuestionEnabled`, resolved to JSON:
   * `{"kind": "ok"|"unknown_question"|"failed"|"busy", "error":
   * string|null}`. */
  setQuestionEnabled(
    seed: string,
    question: string,
    enabled: boolean,
    nowMs: number,
  ): Promise<string>;
  /** #715's switch read. Mirrors `TaskHost::questionSwitches`, resolved to
   * JSON: `{"kind": "ok"|"busy", "switches": [QuestionSwitch]}`. */
  questionSwitches(): string;
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
    deletedAtTouched: boolean,
    deletedAt: number | null,
    nowMs: number,
  ): Promise<string>;
  /** #245's generic pane read. Mirrors `hummingbird-ffi-web`'s
   * `TaskHost::paneRead` — see `RawPaneReadResponse` below for the wire
   * shape, which `client/ffi-web/src/task_host.rs` pins byte-for-byte. */
  paneRead(source: string, nowMs: number): string;
  frontier(): string;
  triageInbox(): string;
  /** Items already grilled once and still foggy — the "triage process"
   * queue's second half (#357). Same `RawItemListResponse` shape as
   * `triageInbox`. */
  grillingItems(): string;
  /** The complete retained roster — see `RawLedgerListResponse`. `nowMs`
   * resolves the alert badge's liveness core-side. */
  ledger(nowMs: number): string;
  /** **Recall** (#478) — see `RawSearchResponse`. `nowMs` resolves the same
   * alert-liveness read `ledger` does; `search` shares its corpus with
   * `ledger`. */
  search(query: string, nowMs: number): string;
  /** Every live `Done` item; same `RawItemListResponse` shape as
   * `frontier`. */
  done(): string;
  blocked(): string;
  /** Items on an external wait (`Stage::Blocked`); same
   * `RawItemListResponse` shape as `frontier`. Pane inputs only — no
   * screen lists these (#675). */
  externallyBlocked(): string;
  steps(itemId: string): string;
  projects(): string;
  /** #624's project create. Mirrors `TaskHost::createProject`, resolved to
   * JSON: `{"kind": "ok"|"failed"|"busy", "id": string|null,
   * "error": string|null}`. The name is trimmed and an empty one refused at
   * the seam, before `Core`. */
  createProject(seed: string, name: string, nowMs: number): Promise<string>;
  /** #625's project patch — the dossier's properties card, and every other
   * project edit. Mirrors `TaskHost::patchProject`, resolved to JSON:
   * `{"kind": "ok"|"failed"|"busy", "error": string|null}`. `currentJson`
   * is the caller's own last-known `Project`, as JSON — the CAS `base` a
   * 409 is diffed against. Each `*Touched` flag distinguishes "leave this
   * field alone" (`false`) from "set it, possibly to `null`" (`true`, with
   * the paired value carrying the new value or `null`) — the same
   * double-`Option` `ProjectPatch` itself carries. `githubRepo`, when
   * touched and non-null, is checked with `is_valid_github_repo` before
   * `Core` is reached. */
  patchProject(
    seed: string,
    currentJson: string,
    name: string | null,
    githubRepoTouched: boolean,
    githubRepo: string | null,
    defaultContextTouched: boolean,
    defaultContext: string | null,
    archivedAtTouched: boolean,
    archivedAt: number | null,
    nowMs: number,
  ): Promise<string>;
  /** #626's per-project link read. Mirrors `TaskHost::projectLinks`,
   * resolved to JSON: `{"kind": "ok"|"busy", "links": [ProjectLink]}`. */
  projectLinks(projectId: string): string;
  /** #626's link create. Mirrors `TaskHost::createProjectLink`, resolved to
   * JSON: `{"kind": "ok"|"failed"|"busy", "id": string|null,
   * "error": string|null}`. The url is trimmed and an empty one refused at
   * the seam, before `Core`. */
  createProjectLink(
    seed: string,
    projectId: string,
    url: string,
    label: string | null,
    position: number,
    nowMs: number,
  ): Promise<string>;
  /** #626's link patch — editing, reordering and removing a link. Mirrors
   * `TaskHost::patchProjectLink`, resolved to JSON:
   * `{"kind": "ok"|"failed"|"busy", "error": string|null}`. `currentJson`
   * is the caller's own last-known `ProjectLink`, as JSON — the CAS `base`
   * a 409 is diffed against. Each `*Touched` flag distinguishes "leave this
   * field alone" (`false`) from "set it, possibly to `null`" (`true`, with
   * the paired value carrying the new value or `null`). */
  patchProjectLink(
    seed: string,
    currentJson: string,
    url: string | null,
    labelTouched: boolean,
    label: string | null,
    position: number | null,
    removedAtTouched: boolean,
    removedAt: number | null,
    nowMs: number,
  ): Promise<string>;
  /** #627's per-project Route read. Mirrors `TaskHost::route`, resolved to
   * JSON: `{"kind": "ok"|"busy", "route": Route|null}`. */
  route(projectId: string): string;
  /** #627's route patch — the dossier's reading column edits
   * destination/notes. Mirrors `TaskHost::patchRoute`, resolved to JSON:
   * `{"kind": "ok"|"failed"|"busy", "error": string|null}`. `currentJson`
   * is the caller's own last-known `Route`, as JSON — the CAS `base` a 409
   * is diffed against. Each `*Touched` flag distinguishes "leave this
   * field alone" (`false`) from "set it, possibly to `null`" (`true`, with
   * the paired value carrying the new value or `null`). */
  patchRoute(
    seed: string,
    currentJson: string,
    destinationTouched: boolean,
    destination: string | null,
    notesTouched: boolean,
    notes: string | null,
    nowMs: number,
  ): Promise<string>;
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

// -- the question off switch (#715, ADR-0034) --------------------------

interface RawQuestionSwitch {
  question: string;
  enabled: boolean;
  pending: boolean;
}

interface RawQuestionSwitchListResponse {
  kind: "ok" | "busy";
  switches: RawQuestionSwitch[];
}

interface RawSetQuestionEnabledResponse {
  kind: "ok" | "unknown_question" | "failed" | "busy";
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
  deleted_at: number | null;
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
  sources: RawSourceOption[];
}

interface RawSourceOption {
  source: string;
  retired_as: string | null;
}

// -- the pane read (#245) — pinned to `PaneReadResponse`'s serde output by
// `client/ffi-web/src/task_host.rs`'s
// `pane_read_response_serializes_with_the_exact_keys_the_pane_shell_ts_parses`.

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
  github_repo: string | null;
  default_context: string | null;
  archived_at: number | null;
  created_at: number;
  updated_at: number;
  version: number;
}

interface RawProjectListResponse {
  kind: "ok" | "busy";
  projects: RawProject[];
  /** #624: the archived half, on the same answer as the live one — an
   * archived project is *absent* in the mirror, so `projects` cannot carry
   * it, and giving the Projects grid a second request for the same read
   * would be a second clock for it. */
  archived: RawProject[];
}

interface RawCreateProjectResponse {
  kind: "ok" | "failed" | "busy";
  id: string | null;
  error: string | null;
}

interface RawPatchProjectResponse {
  kind: "ok" | "failed" | "busy";
  error: string | null;
}

interface RawProjectLink {
  id: string;
  project_id: string;
  url: string;
  label: string | null;
  position: number;
  removed_at: number | null;
  version: number;
}

interface RawProjectLinkListResponse {
  kind: "ok" | "busy";
  links: RawProjectLink[];
}

interface RawCreateProjectLinkResponse {
  kind: "ok" | "failed" | "busy";
  id: string | null;
  error: string | null;
}

interface RawPatchProjectLinkResponse {
  kind: "ok" | "failed" | "busy";
  error: string | null;
}

interface RawRoute {
  project_id: string;
  destination: string | null;
  notes: string | null;
  updated_at: number;
  version: number;
}

interface RawRouteResponse {
  kind: "ok" | "busy";
  route: RawRoute | null;
}

interface RawPatchRouteResponse {
  kind: "ok" | "failed" | "busy";
  error: string | null;
}

interface RawItemListResponse {
  kind: "ok" | "busy";
  items: RawItem[];
}

/** One ledger row: the item's own fields flat at the top level exactly like
 * `RawItem` (`ffi-web`'s `LedgerRowDTO` flattens the same way
 * `FrontierItemDTO` does), plus the row's derivable facts. */
interface RawLedgerRow extends RawItem {
  absent_since_ms: number | null;
  dead_lettered: boolean;
  has_live_alert: boolean;
}

interface RawLedgerListResponse {
  kind: "ok" | "busy";
  rows: RawLedgerRow[];
}

/** One Recall result row: the item's own fields flat at the top level
 * (`ffi-web`'s `SearchRowDTO` flattens the same way `RawItem` does), plus
 * which group it matched in. */
interface RawSearchRow extends RawItem {
  group: RecallGroup;
}

interface RawSearchResponse {
  kind: "ok" | "busy";
  rows: RawSearchRow[];
  total: number;
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

interface RawCompleteGrillResponse {
  kind: "ok" | "not_found" | "item_done" | "needs_re_review" | "failed" | "busy";
  id: string | null;
  error: string | null;
}

interface RawSaveGrillDraftResponse {
  kind: "ok" | "failed" | "busy";
  error: string | null;
}

interface RawDiscardGrillDraftResponse {
  kind: "ok" | "failed" | "busy";
  error: string | null;
}

interface RawGrillDraftResponse {
  kind: "ok" | "busy";
  exists: boolean;
  turns: unknown[] | null;
}

interface RawGrillDraftItemIdsResponse {
  kind: "ok" | "busy";
  item_ids: string[];
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
  entity: string;
  entity_id: string | null;
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

function mapLedgerRow(raw: RawLedgerRow): LedgerRowDTO {
  return {
    ...mapItem(raw),
    absentSinceMs: raw.absent_since_ms,
    deadLettered: raw.dead_lettered,
    hasLiveAlert: raw.has_live_alert,
  };
}

function mapSearchRow(raw: RawSearchRow): RecallRowDTO {
  return {
    ...mapItem(raw),
    group: raw.group,
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

function mapQuestionSwitch(raw: RawQuestionSwitch): QuestionSwitchDTO {
  return { question: raw.question, enabled: raw.enabled, pending: raw.pending };
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
    deletedAt: raw.deleted_at,
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
    sources: raw.sources.map((s) => ({ source: s.source, retiredAs: s.retired_as })),
  };
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
    githubRepo: raw.github_repo,
    defaultContext: raw.default_context,
    archivedAt: raw.archived_at,
    createdAt: raw.created_at,
    updatedAt: raw.updated_at,
    version: raw.version,
  };
}

function mapProjectLink(raw: RawProjectLink): ProjectLinkDTO {
  return {
    id: raw.id,
    projectId: raw.project_id,
    url: raw.url,
    label: raw.label,
    position: raw.position,
    removedAt: raw.removed_at,
    version: raw.version,
  };
}

function mapRoute(raw: RawRoute): RouteDTO {
  return {
    projectId: raw.project_id,
    destination: raw.destination,
    notes: raw.notes,
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

/** `mapStep`'s inverse — a `StepDTO` back to `hummingbird_domain::Step`'s
 * own snake_case field names, for `session_steps`'s JSON array. The Rust
 * side has no `rename_all`, so this is the one seam that must spell the
 * wire's real names rather than lean on `JSON.stringify`'s default
 * camelCase pass-through. */
function unmapStep(step: StepDTO): RawStep {
  return {
    id: step.id,
    item_id: step.itemId,
    body: step.body,
    done: step.done,
    position: step.position,
    deleted_at: step.deletedAt,
    version: step.version,
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
    entity: entry.entity,
    entityId: entry.entity_id,
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
          // Stringified whole, the same idiom `"triage"` uses below — the
          // seam takes one JSON object. A `null` here is "not set" rather
          // than a clear, so unlike triage's edits there is nothing that
          // needs the absent/null distinction to survive the trip.
          JSON.stringify(request.fields),
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
          // `JSON.stringify` is what makes "absent" real across the seam: a
          // key the form never touched is either missing or `undefined`, and
          // both are dropped here, while a deliberate `null` survives.
          JSON.stringify(request.edits),
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
    case "completeGrill": {
      const raw = JSON.parse(
        await host.completeGrill(
          request.seed,
          request.itemId,
          JSON.stringify(request.sessionSteps.map(unmapStep)),
          request.transcript,
          request.summary,
          request.verdict,
          request.modelProposal,
          request.appliedPatch,
          request.deleteUntickedPlan,
          request.nowMs,
        ),
      ) as RawCompleteGrillResponse;
      post({
        type: "completeGrillResult",
        seed: request.seed,
        itemId: request.itemId,
        kind: raw.kind,
        grillId: raw.id,
        error: raw.error,
      });
      return;
    }
    case "saveGrillDraft": {
      const raw = JSON.parse(
        await host.saveGrillDraft(request.itemId, JSON.stringify(request.turns), request.nowMs),
      ) as RawSaveGrillDraftResponse;
      post({
        type: "saveGrillDraftResult",
        itemId: request.itemId,
        kind: raw.kind,
        error: raw.error,
      });
      if (raw.kind === "ok") {
        // A tab's own save is exactly what makes another tab's row learn a
        // draft now exists — #356's "two tabs see one draft, no lock, no
        // arbitration": the broadcast every connected port gets is the
        // whole mechanism, not a second cross-tab channel.
        const idsRaw = JSON.parse(host.grillDraftItemIds()) as RawGrillDraftItemIdsResponse;
        if (idsRaw.kind !== "busy") {
          post({ type: "grillDraftItemIds", itemIds: idsRaw.item_ids });
        }
      }
      return;
    }
    case "discardGrillDraft": {
      const raw = JSON.parse(
        await host.discardGrillDraft(request.itemId, request.nowMs),
      ) as RawDiscardGrillDraftResponse;
      post({
        type: "discardGrillDraftResult",
        itemId: request.itemId,
        kind: raw.kind,
        error: raw.error,
      });
      if (raw.kind === "ok") {
        const idsRaw = JSON.parse(host.grillDraftItemIds()) as RawGrillDraftItemIdsResponse;
        if (idsRaw.kind !== "busy") {
          post({ type: "grillDraftItemIds", itemIds: idsRaw.item_ids });
        }
      }
      return;
    }
    case "getGrillDraft": {
      const raw = JSON.parse(host.grillDraft(request.itemId)) as RawGrillDraftResponse;
      if (raw.kind === "busy") {
        return;
      }
      post({
        type: "grillDraft",
        itemId: request.itemId,
        exists: raw.exists,
        // Opaque, structurally-identical pass-through — this module never
        // interprets a turn's shape, only relays it (same discipline
        // `Core::grill_draft`'s own doc states for the Rust side).
        turns: raw.turns as GrillDraftTurnDTO[] | null,
      });
      return;
    }
    case "getGrillDraftItemIds": {
      const raw = JSON.parse(host.grillDraftItemIds()) as RawGrillDraftItemIdsResponse;
      if (raw.kind === "busy") {
        return;
      }
      post({ type: "grillDraftItemIds", itemIds: raw.item_ids });
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
    case "setQuestionEnabled": {
      const raw = JSON.parse(
        await host.setQuestionEnabled(
          request.seed,
          request.question,
          request.enabled,
          request.nowMs,
        ),
      ) as RawSetQuestionEnabledResponse;
      post({
        type: "setQuestionEnabledResult",
        seed: request.seed,
        question: request.question,
        kind: raw.kind,
        error: raw.error,
      });
      return;
    }
    case "getQuestionSwitches": {
      const raw = JSON.parse(host.questionSwitches()) as RawQuestionSwitchListResponse;
      if (raw.kind === "busy") {
        // No answer, not an empty one — an empty switch list would read as
        // "there are no questions", and a roster defaulting that to all-on
        // would state a fact it had not read. Same contract as
        // `getBindings`.
        return;
      }
      post({ type: "questionSwitches", switches: raw.switches.map(mapQuestionSwitch) });
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
            deleted_at: request.current.deletedAt,
          }),
          request.name,
          request.eventKindTouched,
          request.eventKind,
          request.conditions === null ? null : JSON.stringify(request.conditions),
          request.severity,
          request.tier,
          request.enabled,
          request.deletedAtTouched,
          request.deletedAt,
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
    case "getGrillingItems": {
      const raw = JSON.parse(host.grillingItems()) as RawItemListResponse;
      if (raw.kind === "busy") {
        return;
      }
      post({ type: "grillingItems", items: raw.items.map(mapItem) });
      return;
    }
    case "getLedger": {
      const raw = JSON.parse(host.ledger(request.nowMs)) as RawLedgerListResponse;
      if (raw.kind === "busy") {
        // No answer, not an empty answer — an empty ledger reads as
        // "nothing has ever been tracked" (`lib.rs`'s `BUSY_LEDGER_LIST`).
        return;
      }
      post({ type: "ledger", rows: raw.rows.map(mapLedgerRow) });
      return;
    }
    case "search": {
      const raw = JSON.parse(host.search(request.query, request.nowMs)) as RawSearchResponse;
      if (raw.kind === "busy") {
        // No answer, not an empty answer — same contract as `getLedger`.
        return;
      }
      post({ type: "searchResult", rows: raw.rows.map(mapSearchRow), total: raw.total });
      return;
    }
    case "getDone": {
      const raw = JSON.parse(host.done()) as RawItemListResponse;
      if (raw.kind === "busy") {
        return;
      }
      post({ type: "done", items: raw.items.map(mapItem) });
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
    case "getExternallyBlocked": {
      const raw = JSON.parse(host.externallyBlocked()) as RawItemListResponse;
      if (raw.kind === "busy") {
        return;
      }
      post({ type: "externallyBlocked", items: raw.items.map(mapItem) });
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
      post({
        type: "projects",
        projects: raw.projects.map(mapProject),
        archivedProjects: raw.archived.map(mapProject),
      });
      return;
    }
    case "createProject": {
      const raw = JSON.parse(
        await host.createProject(request.seed, request.name, request.nowMs),
      ) as RawCreateProjectResponse;
      post({
        type: "createProjectResult",
        seed: request.seed,
        kind: raw.kind,
        id: raw.id,
        error: raw.error,
      });
      return;
    }
    case "patchProject": {
      const raw = JSON.parse(
        await host.patchProject(
          request.seed,
          JSON.stringify({
            id: request.current.id,
            name: request.current.name,
            github_repo: request.current.githubRepo,
            default_context: request.current.defaultContext,
            archived_at: request.current.archivedAt,
            created_at: request.current.createdAt,
            updated_at: request.current.updatedAt,
            version: request.current.version,
          }),
          request.name,
          request.githubRepoTouched,
          request.githubRepo,
          request.defaultContextTouched,
          request.defaultContext,
          request.archivedAtTouched,
          request.archivedAt,
          request.nowMs,
        ),
      ) as RawPatchProjectResponse;
      post({
        type: "patchProjectResult",
        seed: request.seed,
        projectId: request.current.id,
        kind: raw.kind,
        error: raw.error,
      });
      return;
    }
    case "getProjectLinks": {
      const raw = JSON.parse(host.projectLinks(request.projectId)) as RawProjectLinkListResponse;
      if (raw.kind === "busy") {
        return;
      }
      post({ type: "projectLinks", projectId: request.projectId, links: raw.links.map(mapProjectLink) });
      return;
    }
    case "createProjectLink": {
      const raw = JSON.parse(
        await host.createProjectLink(
          request.seed,
          request.projectId,
          request.url,
          request.label,
          request.position,
          request.nowMs,
        ),
      ) as RawCreateProjectLinkResponse;
      post({
        type: "createProjectLinkResult",
        seed: request.seed,
        projectId: request.projectId,
        kind: raw.kind,
        id: raw.id,
        error: raw.error,
      });
      return;
    }
    case "patchProjectLink": {
      const raw = JSON.parse(
        await host.patchProjectLink(
          request.seed,
          JSON.stringify({
            id: request.current.id,
            project_id: request.current.projectId,
            url: request.current.url,
            label: request.current.label,
            position: request.current.position,
            removed_at: request.current.removedAt,
            version: request.current.version,
          }),
          request.url,
          request.labelTouched,
          request.label,
          request.position,
          request.removedAtTouched,
          request.removedAt,
          request.nowMs,
        ),
      ) as RawPatchProjectLinkResponse;
      post({
        type: "patchProjectLinkResult",
        seed: request.seed,
        projectId: request.current.projectId,
        kind: raw.kind,
        error: raw.error,
      });
      return;
    }
    case "getRoute": {
      const raw = JSON.parse(host.route(request.projectId)) as RawRouteResponse;
      if (raw.kind === "busy" || raw.route === null) {
        return;
      }
      post({ type: "route", projectId: request.projectId, route: mapRoute(raw.route) });
      return;
    }
    case "patchRoute": {
      const raw = JSON.parse(
        await host.patchRoute(
          request.seed,
          JSON.stringify({
            project_id: request.current.projectId,
            destination: request.current.destination,
            notes: request.current.notes,
            updated_at: request.current.updatedAt,
            version: request.current.version,
          }),
          request.destinationTouched,
          request.destination,
          request.notesTouched,
          request.notes,
          request.nowMs,
        ),
      ) as RawPatchRouteResponse;
      post({
        type: "patchRouteResult",
        seed: request.seed,
        projectId: request.current.projectId,
        kind: raw.kind,
        error: raw.error,
      });
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
