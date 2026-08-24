// The main-thread decision seam (ADR-0025, #141/M1-1).
//
// Every decision ADR-0025 sinks into `hummingbird-core` is reached from the
// web through here, and only through here: `initDecisions` instantiates the
// `hummingbird_ffi_web` wasm module a SECOND time — on the main thread,
// beside the SharedWorker's own instance — and the wrappers below are plain
// synchronous functions over it.
//
// **Why a second instantiation rather than a worker round trip.** The
// modules being sunk run synchronously during React render: capture
// validation on every keystroke, urgency per card, ordering/faceting per
// render off main-thread-only UI state (the grouping axis, the facet
// selection). A `postMessage` hop cannot be spliced into a render, and the
// worker cannot see the state anyway.
//
// **Why that does not touch ADR-0010.** ADR-0010's three failure modes —
// divergent mirrors, two sync timers, duplicate writes — all require a
// second *queue*. Everything reachable through this file is a free function
// over scalars and JSON: it constructs no `Core`, opens no storage, starts
// no timer, and takes `now` as an argument where it needs one.
// `ffi-web`'s `decisions.rs` states the same rule on the Rust side, and the
// ADR-0025 amendment records it as a scope note rather than folklore. The
// invariant to keep is structural: nothing under `src/decisions/` may enter
// `core.worker.ts`'s static import graph (`worker-import-graph.test.ts`
// fails the build if it does).
//
// **Not-ready is a throw, never a fallback.** A TS re-implementation used
// while the module loads would be exactly the second copy this whole plan
// exists to delete — it would go stale silently and be right often enough
// that nobody noticed. `main.tsx` awaits `initDecisions()` before the first
// `createRoot().render()`, and vitest awaits it in `src/test/wasm-setup.ts`,
// so a throw here means a new render path opened that skipped the gate.
// A *failed* `initDecisions()` renders `shell/SeamFailure.tsx` instead of
// `App` for the same reason: there is no error boundary in this app, so
// mounting a decision consumer against a rejected seam would trade a stated
// failure for a blank page on the reader's first capture.

import type { TaskActionName, TaskStageName } from "../store/protocol";

/** The wasm module's shape, named here rather than imported from the
 * generated `.d.ts`: this file is also the type the node-side test loader
 * satisfies, and the generated package is a build artifact (gitignored) the
 * typechecker only sees after `pnpm run build:wasm`. */
import type {
  BindingDTO,
  CalendarEventDTO,
  CalendarReadDTO,
  ConditionDTO,
  FieldTypeName,
  FreshnessDTO,
  KindFieldDTO,
  KindRegistryDTO,
  LedgerRowDTO,
  PaneReadDTO,
  PaneSnapshotDTO,
  ProjectDTO,
  RuleDTO,
  StepDTO,
  TaskItemDTO,
} from "../store/protocol";

export interface DecisionsModule {
  can_submit_capture(draft: string): boolean;
  // M1-2 (#500): the capture decision set.
  compute_urgency(deadline: string | undefined, now: string): string;
  is_valid_deadline(deadline: string): boolean;
  is_valid_scheduled_date(scheduledDate: string): boolean;
  deadline_sort_key(deadline: string): string;
  split_deadline(value: string): string;
  join_deadline(date: string, time: string | undefined): string;
  capture_meta_problems(deadline: string, scheduledDate: string): string;
  priority_from_select(raw: string): number | undefined;
  size_options_json(): string;
  energy_options_json(): string;
  contexts_json(): string;
  frontier_axes_json(): string;
  // M1-3 (#501): the frontier's ordering/grouping/faceting and the
  // combined Now/Triage queue.
  priority_rank(raw: number): number;
  order_frontier_ids(itemsJson: string): string;
  group_frontier_json(itemsJson: string, axis: string, projectsJson: string): string;
  facet_count_json(selectionJson: string): number;
  toggle_facet_json(selectionJson: string, facet: string, value: string): string;
  apply_facets_ids(itemsJson: string, selectionJson: string, now: string): string;
  contexts_of_json(itemsJson: string): string;
  order_triage_ids(itemsJson: string): string;
  triage_process_queue_json(triageJson: string, grillingJson: string, draftIdsJson: string): string;
  // M3 (#532): Done's ordering and the Ledger's ordering + row-state read.
  order_done_ids(itemsJson: string): string;
  ledger_row_state_json(rowJson: string): string;
  ledger_last_touched_ms(rowJson: string): number;
  order_ledger_ids(rowsJson: string): string;
  // M1-4 (#502): the item stage-transition rules.
  item_available_actions(stage: string): string;
  item_applied_stage(action: string): string | undefined;
  item_can_mark_done(stage: string, archived: boolean): boolean;
  item_can_grill(stage: string): boolean;
  item_grill_button_label(hasDraft: boolean): string;
  // M4 (#540): the rules-editor decision set.
  rule_legal_operators_json(fieldType: string): string;
  rule_default_operator(fieldType: string): string | undefined;
  rule_duration_ms(value: string): number | undefined;
  rule_format_duration(amount: number, unit: string): string;
  rule_duration_units_json(fieldType: string): string;
  rule_is_below_alarm_interval(value: string, alarmIntervalMs: number): boolean;
  rule_default_severity(): string;
  rule_fields_for_kind_json(registryJson: string, eventKind: string | undefined): string;
  rule_field_type(
    registryJson: string,
    eventKind: string | undefined,
    fieldName: string,
  ): string | undefined;
  rule_invalid_fields_json(
    registryJson: string,
    eventKind: string | undefined,
    conditionsJson: string,
  ): string;
  rule_is_valid(registryJson: string, eventKind: string | undefined, conditionsJson: string): boolean;
  rule_widget_for(fieldName: string, fieldType: string, operator: string): string;
  rule_new_condition_json(fieldName: string, fieldType: string): string;
  rule_retype_condition_json(conditionJson: string, newFieldType: string): string;
  rule_toggle_negate_json(conditionJson: string): string;
  deadline_picker_datetime(durationValue: string, op: string, now: string): string;
  deadline_picker_duration(inputValue: string, op: string, now: string): string | undefined;
  rule_backtest_ids(
    eventKind: string | undefined,
    conditionsJson: string,
    itemsJson: string,
    nowLocal: string,
    nowUtc: string,
  ): string;
  // M4 (#538): the skills lane's decision half.
  classify_skill_line(text: string): string;
  microtask_result_json(resultJson: string): string;
  grill_result_json(resultJson: string): string;
  reduce_skill_run(stateJson: string, eventJson: string): string;
  reduce_grill_turn(stateJson: string, eventJson: string): string;
  skill_stamp_label(stateJson: string): string | undefined;
  microtask_run_body_json(inputJson: string): string;
  grill_run_body_json(ref: string, turnsJson: string): string;
  format_grill_transcript(turnsJson: string): string;
  decline_for_transport(detail: string): string;
  decline_for_response(status: number): string;
  no_token_decline(): string;
  no_terminal_line_decline(): string;
  outside_schema_decline(): string;
  // #539: the microtask affordance, the backend picker's tier fallback,
  // and the Grill review card's predicates.
  microtask_affordance_json(stepsJson: string): string;
  fallback_backend_id(registryIdsJson: string, deadId: string): string | undefined;
  resolve_backend_selection(stored: string | undefined, registryIdsJson: string): string;
  backend_auto_selection(): string;
  declined_backend_fallback(stateJson: string, selection: string, registryIdsJson: string): string | undefined;
  grill_would_strand_plan(verdict: string, stepsJson: string): boolean;
  grill_plan_replacement_label(stepsJson: string): string;
  grill_demotes_from_frontier(verdict: string, stage: string): boolean;
  grill_frontier_demotion_warning(): string;
  // M4 (#533): the pane shell contract's decided half, the cross-pane
  // sort, the zone bridge, and the waste pane.
  waste_zone_queries_json(inputsJson: string): string;
  waste_facts_json(inputsJson: string, zoneFactsJson: string): string;
  waste_answer_json(inputsJson: string, zoneFactsJson: string): string;
  waste_setup_json(inputsJson: string): string;
  parse_waste_body_json(snapshotJson: string): string;
  pane_zone_queries_json(inputsJson: string, surface: string): string;
  rank_panes_json(inputsJson: string, zoneFactsJson: string, surface: string): string;
  status_alarm_json(inputsJson: string): string;
  order_panes_json(panesJson: string, questionOrderJson: string): string;
  same_pane_identity_json(aJson: string, bJson: string): boolean;
  pane_band_order_json(): string;
  pane_question_order_json(): string;
  waste_constants_json(): string;
  // #535 (M4): the Settings screen's sync-status readout and dead-letter
  // heading.
  sync_status_summary_json(inputJson: string): string;
  dead_letter_heading(count: number): string;
  sync_outcome_class(kind: string): string;
  is_informative_sync_outcome(kind: string): boolean;
  relative_age(ageMs: number): string;
  // #534: the remaining seven panes.
  parse_kimi_body_json(snapshotJson: string): string;
  kimi_facts_json(inputsJson: string): string;
  kimi_answer_json(inputsJson: string): string;
  kimi_band_json(availableBalance: number): string;
  kimi_constants_json(): string;
  parse_workflow_body_json(snapshotJson: string): string;
  github_band_json(bodyJson: string, observedAtMs: number | undefined): string;
  github_observed_at_ms_json(nowMs: number, freshnessJson: string): string;
  github_subjects_json(inputsJson: string): string;
  github_facts_json(subjectKey: string, inputsJson: string): string;
  github_answer_json(subjectKey: string, inputsJson: string): string;
  github_constants_json(): string;
  parse_uptime_body_json(snapshotJson: string): string;
  uptime_band_json(bodyJson: string): string;
  uptime_subjects_json(inputsJson: string): string;
  uptime_facts_json(subjectKey: string, inputsJson: string): string;
  uptime_answer_json(subjectKey: string, inputsJson: string): string;
  uptime_constants_json(): string;
  reachability_facts_json(inputsJson: string): string;
  reachability_answer_json(inputsJson: string): string;
  reachability_constants_json(): string;
  parse_race_body_json(snapshotJson: string): string;
  race_series_from_binding_json(text: string): string;
  race_setup_json(inputsJson: string): string;
  race_subjects_json(inputsJson: string): string;
  next_race_at_json(eventsJson: string, nowMs: number): string;
  race_facts_json(series: string, inputsJson: string): string;
  race_answer_json(subjectKey: string, inputsJson: string): string;
  race_constants_json(): string;
  homework_zone_queries_json(inputsJson: string): string;
  homework_facts_json(inputsJson: string, zoneFactsJson: string): string;
  homework_answer_json(inputsJson: string, zoneFactsJson: string): string;
  homework_link_json(inputsJson: string): string;
  homework_constants_json(): string;
  // #714: the standing-question roster (ADR-0034 decision 4).
  question_roster_json(): string;
  weekend_zone_queries_json(nowMs: number): string;
  weekend_window_json(nowMs: number, zoneFactsJson: string): string;
  weekend_facts_json(inputsJson: string, zoneFactsJson: string): string;
  weekend_answer_json(inputsJson: string, zoneFactsJson: string): string;
  weekend_band_json(windowJson: string, nowMs: number): string;
  weekend_within_band_json(windowJson: string): string;
  weekend_constants_json(): string;
  vacation_zone_queries_json(inputsJson: string): string;
  vacation_setup_json(inputsJson: string): string;
  trip_queue_json(eventsJson: string, calendarId: string, today: string, zoneFactsJson: string): string;
  vacation_band_json(nextTripJson: string): string;
  vacation_view_json(inputsJson: string, zoneFactsJson: string): string;
  vacation_answer_json(inputsJson: string, zoneFactsJson: string): string;
  vacation_constants_json(): string;
  scps_zone_queries_json(inputsJson: string): string;
  scps_view_json(inputsJson: string, zoneFactsJson: string): string;
  scps_answer_json(inputsJson: string, zoneFactsJson: string): string;
  scps_constants_json(): string;
  device_zone(): string;
}

let loaded: DecisionsModule | null = null;
let inFlight: Promise<DecisionsModule> | null = null;

/** The browser loader: the same wasm-pack `--target bundler` package the
 * SharedWorker imports, resolved by `vite-plugin-wasm` +
 * `vite-plugin-top-level-await`. A dynamic import, so nothing pulls the
 * binary in at module-evaluation time and a test that supplies its own
 * loader never touches this path. */
async function loadInBrowser(): Promise<DecisionsModule> {
  return (await import("../wasm/pkg/hummingbird_ffi_web")) as DecisionsModule;
}

/** Instantiate the decision module. Idempotent and concurrency-safe: the
 * second caller gets the first caller's promise, never a second
 * instantiation. `load` is the seam's own seam — vitest passes a node
 * loader (`src/test/wasm-setup.ts`), production passes nothing. */
export function initDecisions(load: () => Promise<DecisionsModule> = loadInBrowser): Promise<void> {
  if (loaded) return Promise.resolve();
  const startedAt = performance?.now?.() ?? 0;
  inFlight ??= load().then((module) => {
    loaded = module;
    // The gate's own cost, on the timeline the browser already keeps —
    // this is the number M1-1's flip condition (">~300 ms p50 added to the
    // loading gate") is written against, and the later M1 issues re-read it
    // as they sink more decisions rather than re-deriving a harness. A
    // `PerformanceMeasure`, not a `console.log`: it is readable from
    // devtools and from a Playwright run, and costs nothing when nobody
    // looks. `performance` is absent in no environment this file runs in
    // except a bare node one, hence the guard.
    performance?.measure?.("hb:decisions-init", { start: startedAt });
    return module;
  });
  return inFlight.then(() => undefined);
}

/** Whether the synchronous wrappers below can be called — the App-side half
 * of the loading gate. */
export function decisionsReady(): boolean {
  return loaded !== null;
}

/** Test-only reset, so a test can prove the not-ready throw without leaking
 * a poisoned module into the rest of the file's suite. Exported rather than
 * reached through a mock because the state it clears is this module's own. */
export function resetDecisionsForTest(): void {
  loaded = null;
  inFlight = null;
}

function required(): DecisionsModule {
  if (!loaded) {
    throw new Error(
      "decision seam used before initDecisions() resolved — see src/decisions/seam.ts",
    );
  }
  return loaded;
}

/** Whether `draft` is a real capture worth submitting (#110/S12). The rule
 * itself is `hummingbird_core::decisions::can_submit_capture`; this is the
 * call, and nothing else. */
export function canSubmitCapture(draft: string): boolean {
  return required().can_submit_capture(draft);
}


// ------------------------------------------------------------ M1-2 (#500)
// The capture decision set: urgency, the deadline-field grammar, and the
// capture/triage field problems. `urgency.ts`, `deadline-parts.ts` and the
// decision half of `capture-meta.ts` are re-exports of the wrappers below —
// see those files for why `field-vocabulary.ts`'s own exports stay literal
// TS arrays rather than calling `sizeOptions`/`energyOptions`/`contexts`
// here directly (a module-evaluation-order constraint: those arrays are
// read at React-render time by components that are statically imported —
// and so, transitively, MODULE-EVALUATED — before `initDecisions()`
// resolves; a top-level `const` computed by calling into wasm at that point
// would throw the "used before ready" guard on every page load, per this
// file's own "not-ready is a throw, never a fallback" rule above).

export type Urgency = "calm" | "soon" | "now" | "overdue";

/** The rule itself is `hummingbird_core::decisions::urgency::compute_urgency`.
 * `nowMs` is a real epoch millisecond count (`Date.now()`), exactly as
 * every existing caller already holds one; this wrapper — not the core, per
 * ADR-0015's "resolves no civil date to an instant" rule — is what turns it
 * into the deadline-shaped local wall-clock string the Rust function
 * takes. */
export function computeUrgency(deadline: string | null, nowMs: number): Urgency {
  return required().compute_urgency(deadline ?? undefined, localWallClock(nowMs)) as Urgency;
}

/** Renders `nowMs` as this device's own local wall-clock reading, in the
 * deadline grammar's own shape (`YYYY-MM-DDTHH:MM`) — the one place a
 * timezone offset is read on the web, matching exactly what the retired
 * `computeUrgency`'s `new Date(year, month, day, hour, minute)` call used
 * to do implicitly on both sides of its subtraction. */
function localWallClock(nowMs: number): string {
  const d = new Date(nowMs);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function isValidDeadline(deadline: string): boolean {
  return required().is_valid_deadline(deadline);
}

export function isValidScheduledDate(scheduledDate: string): boolean {
  return required().is_valid_scheduled_date(scheduledDate);
}

export function deadlineSortKey(deadline: string): string {
  return required().deadline_sort_key(deadline);
}

export interface DeadlineParts {
  date: string;
  time: string | null;
}

export function splitDeadline(value: string): DeadlineParts {
  return JSON.parse(required().split_deadline(value)) as DeadlineParts;
}

export function joinDeadline(date: string, time: string | null): string {
  return required().join_deadline(date, time ?? undefined);
}

export interface CaptureMetaProblems {
  deadline?: string;
  scheduledDate?: string;
}

/** `hummingbird_core::decisions::capture::capture_meta_problems` — shared
 * verbatim by `capture-meta.ts`'s `captureMetaProblems` and
 * `triage-form.ts`'s `triageDraftProblems`, which used to hand-copy the
 * same two messages. */
export function captureMetaProblems(deadline: string, scheduledDate: string): CaptureMetaProblems {
  const raw = JSON.parse(required().capture_meta_problems(deadline, scheduledDate)) as {
    deadline: string | null;
    scheduledDate: string | null;
  };
  const problems: CaptureMetaProblems = {};
  if (raw.deadline !== null) problems.deadline = raw.deadline;
  if (raw.scheduledDate !== null) problems.scheduledDate = raw.scheduledDate;
  return problems;
}

/** `hummingbird_core::decisions::capture::priority_from_select` — the
 * capture box's `"0"` -> "not sent" priority rule. */
export function priorityFromSelect(raw: string): number | null {
  return required().priority_from_select(raw) ?? null;
}

export interface VocabOption {
  value: string;
  label: string;
}

/** `hummingbird_core::decisions::vocabulary::size_options` /
 * `energy_options` — the vocabulary's real values only, no leading "Not
 * set" entry. Not yet called by any production module in M1-2: see the
 * header above for why `field-vocabulary.ts` keeps its own literal arrays
 * for now, and `field-vocabulary.test.ts` for the pinning test that keeps
 * them provably equal to these. */
export function sizeOptionsFromCore(): VocabOption[] {
  return JSON.parse(required().size_options_json()) as VocabOption[];
}

export function energyOptionsFromCore(): VocabOption[] {
  return JSON.parse(required().energy_options_json()) as VocabOption[];
}

/** `hummingbird_core::decisions::vocabulary::CONTEXTS` — pinning-test-only
 * in M1-2 for the same reason as the two functions above. */
export function contextsFromCore(): string[] {
  return JSON.parse(required().contexts_json()) as string[];
}

/** `hummingbird_core::decisions::vocabulary::FRONTIER_AXES` — M1-3's
 * (#501) first consumer; nothing in M1-2 calls this in production. */
export function frontierAxesFromCore(): string[] {
  return JSON.parse(required().frontier_axes_json()) as string[];
}

/** `hummingbird_core::decisions::frontier::priority_rank` — pinning-test-only:
 * `screens/priority.ts`'s `priorityRank` is the module-evaluation-time
 * literal every production caller uses (the frontier's own order rendered
 * inline, before the seam is guaranteed ready), and `seam.test.ts` pins it
 * against this wrapper so the two copies cannot drift silently. */
export function priorityRankFromCore(raw: number): number {
  return required().priority_rank(raw);
}

// ------------------------------------------------------------ M1-3 (#501)
// The frontier's ordering, grouping and faceting, and the combined
// Now/Triage queue. `frontier-order.ts`, `frontier-columns.ts`,
// `frontier-facets.ts`, `triage-order.ts` and `triage-process-order.ts` are
// all re-exports of the wrappers below — every one of *their* exports was
// already a function called from event handlers and render bodies, never at
// module-evaluation time, so (per `field-vocabulary.ts`'s header) the seam
// call is safe everywhere they made it.
//
// **Ids cross the boundary, not whole items.** The wasm side already holds
// nothing about the item beyond what a rule reads (id, priority, deadline,
// context, size, energy, projectId — `FrontierItemDTO` in
// `ffi-web/src/decisions.rs`, exactly `QueueItemDTO`'s pattern below it);
// the caller here holds the full `TaskItemDTO` already, so every wrapper
// below re-serializes only that seven-field slice outward via
// `frontierPayload` and maps the ordered/filtered ids it gets back onto its
// own items — never round-tripping a whole item through JSON in either
// direction.

/** The seven fields `hummingbird_core::decisions::frontier::FrontierItem`
 * reads — what actually crosses into wasm, camelCase already, and nothing
 * else `TaskItemDTO` carries (no title, no stage, no timestamps): matches
 * `FrontierItemDTO` in `ffi-web/src/decisions.rs` field for field. */
function frontierPayload(items: readonly TaskItemDTO[]): string {
  return JSON.stringify(
    items.map((item) => ({
      id: item.id,
      priority: item.priority,
      deadline: item.deadline,
      context: item.context,
      size: item.size,
      energy: item.energy,
      projectId: item.projectId,
    })),
  );
}

function byIdMap(items: readonly TaskItemDTO[]): Map<string, TaskItemDTO> {
  return new Map(items.map((item) => [item.id, item]));
}

/** `hummingbird_core::decisions::frontier`'s stable display order: most
 * urgent priority first, then soonest deadline, then id as the tie-break. */
export function orderFrontier(items: readonly TaskItemDTO[]): TaskItemDTO[] {
  const ids = JSON.parse(required().order_frontier_ids(frontierPayload(items))) as string[];
  const byId = byIdMap(items);
  return ids.map((id) => byId.get(id)!);
}

/** The axes the frontier can be grouped by (ADR-0021 decision 1) —
 * `hummingbird_core::decisions::frontier::FrontierAxis`'s own wire names. */
export type FrontierAxis = "context" | "project" | "size" | "energy";

/** Every axis, in the order the switch offers them — pinned against
 * `hummingbird_core::decisions::frontier::FRONTIER_GROUP_AXES` by
 * `frontier-columns.test.ts`. A literal array, not a seam call: this is
 * read at module-evaluation time by every static importer of
 * `frontier-columns.ts` (`frontier-prefs.ts`, `FrontierColumns.tsx`), which
 * happens before `initDecisions()` ever resolves — see `field-vocabulary
 * .ts`'s header for the full argument. */
export const FRONTIER_AXES: readonly FrontierAxis[] = ["context", "project", "size", "energy"];

export const DEFAULT_FRONTIER_AXIS: FrontierAxis = "context";

export interface FrontierColumn {
  value: string | null;
  label: string | null;
  items: TaskItemDTO[];
}

/** `hummingbird_core::decisions::frontier::group_frontier` — fullest column
 * first, the no-value column always last, within-column order preserved
 * from `items`. */
export function groupFrontier(
  items: readonly TaskItemDTO[],
  axis: FrontierAxis,
  projects: readonly ProjectDTO[],
): FrontierColumn[] {
  const byId = byIdMap(items);
  const projectsJson = JSON.stringify(
    projects.map((project) => ({ id: project.id, name: project.name })),
  );
  const raw = JSON.parse(
    required().group_frontier_json(frontierPayload(items), axis, projectsJson),
  ) as Array<{ value: string | null; label: string | null; ids: string[] }>;
  return raw.map((column) => ({
    value: column.value,
    label: column.label,
    items: column.ids.map((id) => byId.get(id)!),
  }));
}

/** The frontier's facet filter — `hummingbird_core::decisions::frontier
 * ::Facet`'s own wire names, exactly `FRONTIER_AXES` from
 * `vocabulary.rs` (urgency in place of project: colour already carries
 * urgency across whatever grouping axis is live, and a project column
 * already isolates one project). */
export type Facet = "context" | "size" | "energy" | "urgency";

/** Pinned against `hummingbird_core::decisions::vocabulary::FRONTIER_AXES`
 * by `frontier-facets.test.ts` — a literal array for the same
 * module-evaluation-order reason as `FRONTIER_AXES` above. */
export const FACETS: readonly Facet[] = ["context", "size", "energy", "urgency"];

/** `hummingbird_domain::Size::ALL`'s wire values, in order — pinned against
 * `sizeOptionsFromCore()` by `frontier-facets.test.ts` rather than derived
 * from it directly, for the same module-evaluation-order reason
 * `field-vocabulary.ts`'s own literal arrays stay literal. */
export const SIZES: readonly string[] = ["quick", "normal", "deep"];

/** `hummingbird_domain::Energy::ALL`'s wire values, in order — pinned the
 * same way as `SIZES`. */
export const ENERGIES: readonly string[] = ["low", "medium", "high"];

/** `calm` is absent: it is the default, and a facet for "nothing pressing"
 * is a facet for "everything", which the unpicked state already means. */
export const URGENCIES: readonly Urgency[] = ["overdue", "now", "soon"];

/** Display token for the column and chip of items naming no value —
 * `hummingbird_core::decisions::frontier::NO_CONTEXT` verbatim. */
export const NO_CONTEXT = "no context";

export type FacetSelection = Readonly<Record<Facet, ReadonlySet<string>>>;

export const NO_FACETS: FacetSelection = {
  context: new Set(),
  size: new Set(),
  energy: new Set(),
  urgency: new Set(),
};

interface FacetSelectionWire {
  context: string[];
  size: string[];
  energy: string[];
  urgency: string[];
}

function toFacetSelectionWire(picked: FacetSelection): FacetSelectionWire {
  return {
    context: [...picked.context],
    size: [...picked.size],
    energy: [...picked.energy],
    urgency: [...picked.urgency],
  };
}

function fromFacetSelectionWire(wire: FacetSelectionWire): FacetSelection {
  return {
    context: new Set(wire.context),
    size: new Set(wire.size),
    energy: new Set(wire.energy),
    urgency: new Set(wire.urgency),
  };
}

/** `hummingbird_core::decisions::frontier::facet_count` — how many values
 * are picked across every facet, the Filter button's badge. */
export function facetCount(picked: FacetSelection): number {
  return required().facet_count_json(JSON.stringify(toFacetSelectionWire(picked)));
}

/** `hummingbird_core::decisions::frontier::toggle_facet`. */
export function toggleFacet(picked: FacetSelection, facet: Facet, value: string): FacetSelection {
  const wire = JSON.parse(
    required().toggle_facet_json(JSON.stringify(toFacetSelectionWire(picked)), facet, value),
  ) as FacetSelectionWire;
  return fromFacetSelectionWire(wire);
}

/** `hummingbird_core::decisions::frontier::matches_facets`, over one item —
 * derived from `applyFacets` on a one-item array rather than a second
 * wasm export, since no production caller needs the single-item form on
 * its own critical path (`FrontierColumns.tsx` always filters a whole
 * frontier at once). */
export function matchesFacets(item: TaskItemDTO, picked: FacetSelection, nowMs: number): boolean {
  return applyFacets([item], picked, nowMs).some((matched) => matched.id === item.id);
}

/** `hummingbird_core::decisions::frontier::apply_facets` — the picked
 * items, in the order given. */
export function applyFacets(
  items: readonly TaskItemDTO[],
  picked: FacetSelection,
  nowMs: number,
): TaskItemDTO[] {
  const byId = byIdMap(items);
  const ids = JSON.parse(
    required().apply_facets_ids(
      frontierPayload(items),
      JSON.stringify(toFacetSelectionWire(picked)),
      localWallClock(nowMs),
    ),
  ) as string[];
  return ids.map((id) => byId.get(id)!);
}

/** `hummingbird_core::decisions::frontier::contexts_of` — contexts actually
 * present in the given items, suggested vocabulary first, extras sorted,
 * `NO_CONTEXT` last. */
export function contextsOf(items: readonly TaskItemDTO[]): string[] {
  return JSON.parse(required().contexts_of_json(frontierPayload(items))) as string[];
}

function queuePayload(items: readonly TaskItemDTO[]): string {
  return JSON.stringify(items.map((item) => ({ id: item.id, createdAt: item.createdAt })));
}

/** `hummingbird_core::decisions::queue::order_triage` — oldest capture
 * first, id as the tie-break. */
export function orderTriage(items: readonly TaskItemDTO[]): TaskItemDTO[] {
  const ids = JSON.parse(required().order_triage_ids(queuePayload(items))) as string[];
  const byId = byIdMap(items);
  return ids.map((id) => byId.get(id)!);
}

export interface TriageProcessQueue {
  items: TaskItemDTO[];
  capturedCount: number;
  grillingCount: number;
}

/** `hummingbird_core::decisions::queue::triage_process_queue` — the one
 * function deciding Now/Triage membership and order: local drafts first,
 * then Grilling-stage items, then captured Triage items. */
export function triageProcessQueue(
  triageItems: readonly TaskItemDTO[],
  grillingItems: readonly TaskItemDTO[],
  draftItemIds: readonly string[],
): TriageProcessQueue {
  const byId = byIdMap([...triageItems, ...grillingItems]);
  const raw = JSON.parse(
    required().triage_process_queue_json(
      queuePayload(triageItems),
      queuePayload(grillingItems),
      JSON.stringify(draftItemIds),
    ),
  ) as { ids: string[]; capturedCount: number; grillingCount: number };
  return {
    items: raw.ids.map((id) => byId.get(id)!),
    capturedCount: raw.capturedCount,
    grillingCount: raw.grillingCount,
  };
}

// -------------------------------------------------------------- M3 (#532)
// Done's ordering and the Ledger's ordering + row-state read, sunk from
// `screens/done-order.ts` and `screens/ledger-order.ts` into
// `hummingbird_core::decisions::roster`. Both files are now seam
// re-exports of the wrappers below.

function rosterPayload(items: readonly TaskItemDTO[]): string {
  return JSON.stringify(items.map((item) => ({ id: item.id, updatedAt: item.updatedAt })));
}

/** `hummingbird_core::decisions::roster::order_done` — most recently
 * touched first, id as the tie-break. */
export function orderDone(items: readonly TaskItemDTO[]): TaskItemDTO[] {
  const ids = JSON.parse(required().order_done_ids(rosterPayload(items))) as string[];
  const byId = byIdMap(items);
  return ids.map((id) => byId.get(id)!);
}

type LedgerRosterFields = Pick<LedgerRowDTO, "id" | "updatedAt" | "archivedAt" | "absentSinceMs">;

function ledgerRosterFields(row: LedgerRosterFields): LedgerRosterFields {
  return {
    id: row.id,
    updatedAt: row.updatedAt,
    archivedAt: row.archivedAt,
    absentSinceMs: row.absentSinceMs,
  };
}

function ledgerRosterPayload(row: LedgerRosterFields): string {
  return JSON.stringify(ledgerRosterFields(row));
}

/** `ledger-order.ts`'s own union, verbatim —
 * `hummingbird_core::decisions::roster::LedgerRowState`. */
export type LedgerRowState = { kind: "live" } | { kind: "archived"; sinceMs: number };

/** `hummingbird_core::decisions::roster::ledger_row_state`. */
export function ledgerRowState(row: LedgerRowDTO): LedgerRowState {
  return JSON.parse(required().ledger_row_state_json(ledgerRosterPayload(row))) as LedgerRowState;
}

/** `hummingbird_core::decisions::roster::last_touched_ms`. */
export function lastTouchedMs(row: LedgerRowDTO): number {
  return required().ledger_last_touched_ms(ledgerRosterPayload(row));
}

/** `hummingbird_core::decisions::roster::order_ledger` — last touched
 * first, id as the tie-break. */
export function orderLedger(rows: readonly LedgerRowDTO[]): LedgerRowDTO[] {
  const payload = JSON.stringify(rows.map((row) => ledgerRosterFields(row)));
  const ids = JSON.parse(required().order_ledger_ids(payload)) as string[];
  const byId = new Map(rows.map((row) => [row.id, row]));
  return ids.map((id) => byId.get(id)!);
}

// ------------------------------------------------------------ M1-4 (#502)

/** S11/#109's act affordances for `stage` — `hummingbird_core::decisions::available_actions`
 * (M1-4, #502) through the wasm boundary's JSON array. */
export function availableActions(stage: TaskStageName): readonly TaskActionName[] {
  return JSON.parse(required().item_available_actions(stage)) as TaskActionName[];
}

/** The stage an act vocabulary word sets, or `null` for `"cancel"` (which
 * touches `archivedAt` instead of `stage`) —
 * `hummingbird_core::decisions::applied_stage` verbatim. */
export function appliedStage(action: TaskActionName): TaskStageName | null {
  return (required().item_applied_stage(action) as TaskStageName | undefined) ?? null;
}

/** Whether a row offers the one-click "mark done" checkmark —
 * `hummingbird_core::decisions::can_mark_done` verbatim. */
export function canMarkDone(stage: TaskStageName, archived: boolean): boolean {
  return required().item_can_mark_done(stage, archived);
}

/** Whether a row offers "Grill me" —
 * `hummingbird_core::decisions::can_grill` verbatim. */
export function canGrill(stage: TaskStageName): boolean {
  return required().item_can_grill(stage);
}

/** The Grill button's own label —
 * `hummingbird_core::decisions::grill_button_label` verbatim. */
export function grillButtonLabel(hasDraft: boolean): "Grill me" | "Resume grill" {
  return required().item_grill_button_label(hasDraft) as "Grill me" | "Resume grill";
}

// ------------------------------------------------------------- M4 (#540)
// The rules-editor decision set: the operator table, the duration grammar,
// the kind → field cascade, the validity read, the `deadline` picker, the
// condition-row widget cascade and the backtest. `screens/rules/`'s seven
// modules are re-exports of the wrappers below — see
// `hummingbird_core::decisions::rules`'s own `mod.rs` for what the sink
// retires, including the two drifts ADR-0025's M1 verdict table recorded as
// debt (`rules/backtest.ts:52`, `rules/deadline-picker.ts:32`).
//
// Every export of those modules was already a function called from event
// handlers and render bodies, never at module-evaluation time, so (per
// `field-vocabulary.ts`'s header) the seam call is safe everywhere they
// made it. The two things that stayed TS are renderings, not decisions:
// `OPERATOR_LABELS` and `kindLabel`/`kindOptions`.

export type OperatorName = "eq" | "contains" | "gt" | "lt" | "is" | "within_next" | "within_last";

export type DurationUnit = "m" | "h" | "d";

export type ValueWidget =
  | "chips"
  | "duration"
  | "datetime"
  | "boolean"
  | "number"
  /** `source` under `eq`: a pick from the registry's frozen vocabulary
   * rather than a text box a typo makes silently unmatchable. */
  | "source"
  | "text";

export type DeadlineOperator = "within_next" | "within_last";

/** `hummingbird_core::decisions::rules::legal_operators` — derived from
 * `hummingbird_rules_engine::Operator::is_legal_for`, so the dropdown and
 * the authority's own gate cannot drift. */
export function legalOperators(fieldType: FieldTypeName): OperatorName[] {
  return JSON.parse(required().rule_legal_operators_json(fieldType)) as OperatorName[];
}

/** `hummingbird_core::decisions::rules::default_operator_for` — always the
 * first of `legalOperators`, so a newly added row is never illegal. */
export function defaultOperatorFor(fieldType: FieldTypeName): OperatorName {
  return required().rule_default_operator(fieldType) as OperatorName;
}

/** `hummingbird_core::decisions::rules::parse_duration_ms`. */
export function parseDurationMs(value: string): number | undefined {
  return required().rule_duration_ms(value);
}

/** `hummingbird_core::decisions::rules::format_duration` —
 * `parseDurationMs`'s inverse. */
export function formatDuration(amount: number, unit: DurationUnit): string {
  return required().rule_format_duration(amount, unit);
}

/** `hummingbird_core::decisions::rules::duration_units_for` — ADR-0013's
 * own table: a `date` field is day-grained only. */
export function durationUnitsFor(fieldType: "timestamp" | "date"): DurationUnit[] {
  return JSON.parse(required().rule_duration_units_json(fieldType)) as DurationUnit[];
}

/** `hummingbird_core::decisions::rules::is_below_alarm_interval` — the
 * duration warning (#138). Warn, never reject. */
export function isBelowAlarmInterval(value: string, alarmIntervalMs: number): boolean {
  return required().rule_is_below_alarm_interval(value, alarmIntervalMs);
}

/** `hummingbird_core::decisions::rules::DEFAULT_SEVERITY` — the severity a
 * fresh draft opens on, which the phone's form reads from the same const. */
export function defaultSeverity(): string {
  return required().rule_default_severity();
}

/** The registry crosses in full on every call: it is five kinds, the caller
 * already holds it from the `kindRegistry` push, and passing it keeps the
 * client editing against the catalogue its *authority* exported rather than
 * the one the wasm binary happened to compile (`rules::validity`'s header
 * argues this at length). */
function registryPayload(registry: KindRegistryDTO): string {
  return JSON.stringify(registry);
}

/** `hummingbird_core::decisions::rules::fields_for_kind` — the Event core
 * for "any kind", core-first-then-the-kind's-own for a named one, never a
 * core name listed twice. */
export function fieldsForKind(registry: KindRegistryDTO, eventKind: string | null): KindFieldDTO[] {
  return JSON.parse(
    required().rule_fields_for_kind_json(registryPayload(registry), eventKind ?? undefined),
  ) as KindFieldDTO[];
}

/** `hummingbird_core::decisions::rules::field_type` — `undefined` for a
 * field outside the list `eventKind` offers. */
export function fieldType(
  registry: KindRegistryDTO,
  eventKind: string | null,
  fieldName: string,
): FieldTypeName | undefined {
  return required().rule_field_type(
    registryPayload(registry),
    eventKind ?? undefined,
    fieldName,
  ) as FieldTypeName | undefined;
}

export interface RuleInvalidField {
  field: string;
}

/** `hummingbird_core::decisions::rules::invalid_fields` — every condition
 * field the rule's kind no longer declares. Display-only: it never blocks a
 * save (#133's `validate_rule` does that, server-side) and never mutates
 * the rule. */
export function invalidFields(
  rule: Pick<RuleDTO, "eventKind" | "conditions">,
  registry: KindRegistryDTO,
): RuleInvalidField[] {
  const names = JSON.parse(
    required().rule_invalid_fields_json(
      registryPayload(registry),
      rule.eventKind ?? undefined,
      JSON.stringify(rule.conditions),
    ),
  ) as string[];
  return names.map((field) => ({ field }));
}

/** `hummingbird_core::decisions::rules::is_rule_valid` — the boolean an
 * invalid-rule badge gates on. */
export function isRuleValid(
  rule: Pick<RuleDTO, "eventKind" | "conditions">,
  registry: KindRegistryDTO,
): boolean {
  return required().rule_is_valid(
    registryPayload(registry),
    rule.eventKind ?? undefined,
    JSON.stringify(rule.conditions),
  );
}

/** `hummingbird_core::decisions::rules::widget_for` — the value control one
 * condition row offers, `deadline`'s date/time picker included. */
export function widgetFor(
  fieldName: string,
  fieldTypeName: FieldTypeName,
  operator: OperatorName,
): ValueWidget {
  return required().rule_widget_for(fieldName, fieldTypeName, operator) as ValueWidget;
}

/** `hummingbird_core::decisions::rules::new_condition`. */
export function newCondition(fieldName: string, fieldTypeName: FieldTypeName): ConditionDTO {
  return JSON.parse(required().rule_new_condition_json(fieldName, fieldTypeName)) as ConditionDTO;
}

/** `hummingbird_core::decisions::rules::retype_condition`.
 *
 * The Rust side answers `null` for "already legal, leave it exactly as it
 * is" rather than echoing a structurally equal copy — so this returns the
 * caller's *own* object in that case, and a React caller keyed on identity
 * does not re-render on a field pick that changed nothing. The decision is
 * still entirely Rust-side; only the identity is preserved here. */
export function retypeCondition(condition: ConditionDTO, newFieldType: FieldTypeName): ConditionDTO {
  const retyped = JSON.parse(
    required().rule_retype_condition_json(JSON.stringify(condition), newFieldType),
  ) as ConditionDTO | null;
  return retyped ?? condition;
}

/** `hummingbird_core::decisions::rules::toggle_negate` — the per-row "not"
 * toggle, and nothing else. */
export function toggleNegate(condition: ConditionDTO): ConditionDTO {
  return JSON.parse(required().rule_toggle_negate_json(JSON.stringify(condition))) as ConditionDTO;
}

/** `hummingbird_core::decisions::rules::datetime_input_value_from_duration`
 * — the `datetime-local` value that displays a stored duration as a
 * concrete moment. `nowMs` is a real epoch millisecond count; this wrapper,
 * not the core, resolves it into the device's own wall clock. */
export function datetimeInputValueFromDuration(
  durationValue: string,
  op: DeadlineOperator,
  nowMs: number,
): string {
  return required().deadline_picker_datetime(durationValue, op, localWallClock(nowMs));
}

/** `hummingbird_core::decisions::rules::duration_from_datetime_input_value`
 * — the wire duration literal for a picked moment, in whole minutes. */
export function durationFromDatetimeInputValue(
  inputValue: string,
  op: DeadlineOperator,
  nowMs: number,
): string | undefined {
  return required().deadline_picker_duration(inputValue, op, localWallClock(nowMs));
}

export type BacktestUnavailableReason = "no_local_history";

export type BacktestResult =
  | { kind: "unavailable"; reason: BacktestUnavailableReason }
  | { kind: "ok"; matches: TaskItemDTO[] };

/** `nowMs` in UTC, in the deadline grammar's own shape — `localWallClock`'s
 * twin, and the TS side of `hummingbird_domain::now_as_deadline`
 * (minute-precision, seconds truncated never rounded). The backtest needs
 * both readings of the one instant because `occurred_at` is stamped UTC by
 * the authority while `deadline`/`scheduled_date` are device-local civil
 * strings; see `rules::backtest`'s header for why the frames are named at
 * this boundary rather than inferred inside a crate with no tzdb. */
function utcWallClock(nowMs: number): string {
  const d = new Date(Math.floor(nowMs / 60_000) * 60_000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}T${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`;
}

/** `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM` — `hummingbird_domain::is_valid_deadline`'s
 * shape, tested here only to decide whether a stored value needs resolving
 * (the real validation stays Rust-side). */
const DEADLINE_SHAPE = /^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2})?$/;

/** A stored `deadline`/`scheduledDate`, in the civil local frame the core
 * compares against.
 *
 * Anything already deadline-shaped passes through untouched, which is every
 * value the authority accepts. A value carrying its own zone designator (a
 * trailing `Z`, an offset) is *this* layer's to resolve — reading a zone is
 * exactly what the core cannot do and the host can — so it is rendered into
 * the device's own wall clock, the same reading the retired
 * `backtest.ts`'s bare `new Date(...)` gave it. Anything neither shaped nor
 * parseable passes through and simply never matches, as it did before. */
function localCivil(value: string): string {
  if (DEADLINE_SHAPE.test(value)) return value;
  const ms = new Date(value).getTime();
  return Number.isNaN(ms) ? value : localWallClock(ms);
}

/** The fifteen fields `hummingbird_core::decisions::rules::BacktestItem`
 * reads — exactly the field set `authority::sweep::item_threshold_event`
 * populates, camelCase, and nothing else `TaskItemDTO` carries. Matches
 * `BacktestItemDTO` in `ffi-web/src/decisions.rs` field for field. */
function backtestPayload(items: readonly TaskItemDTO[]): string {
  return JSON.stringify(
    items.map((item) => ({
      id: item.id,
      // `item_threshold_event` sets `occurred_at: now_as_deadline(item.updated_at)`
      // — a poll-time-derived core field, not a stored one, but derivable
      // exactly from the same `updatedAt` this DTO already carries.
      occurredAt: utcWallClock(item.updatedAt),
      title: item.title,
      body: item.description ?? undefined,
      url: item.sourceUrl ?? undefined,
      deadline: item.deadline === null ? undefined : localCivil(item.deadline),
      scheduledDate: item.scheduledDate === null ? undefined : localCivil(item.scheduledDate),
      stage: item.stage,
      size: item.size ?? undefined,
      energy: item.energy ?? undefined,
      context: item.context ?? undefined,
      priority: item.priority,
      projectId: item.projectId ?? undefined,
      source: item.source ?? undefined,
      sourceKey: item.sourceKey ?? undefined,
    })),
  );
}

/** `hummingbird_core::decisions::rules::backtest` — which of `items` a
 * draft rule would have promoted (ADR-0011). Pure: it writes nothing and
 * calls nothing, and the ids come back rather than whole items (the caller
 * holds those already), exactly the frontier wrappers' pattern. */
export function backtest(
  rule: Pick<RuleDTO, "eventKind" | "conditions">,
  items: readonly TaskItemDTO[],
  nowMs: number,
): BacktestResult {
  const raw = JSON.parse(
    required().rule_backtest_ids(
      rule.eventKind ?? undefined,
      JSON.stringify(rule.conditions),
      backtestPayload(items),
      localWallClock(nowMs),
      utcWallClock(nowMs),
    ),
  ) as { kind: "unavailable"; reason: BacktestUnavailableReason } | { kind: "ok"; ids: string[] };
  if (raw.kind === "unavailable") {
    return { kind: "unavailable", reason: raw.reason };
  }
  const byId = byIdMap(items);
  return { kind: "ok", matches: raw.ids.map((id) => byId.get(id)!) };
}

// -------------------------------------------------------------- M4 (#538)
// The skills lane's decision half — `hummingbird_core::decisions::skills`,
// which `client/web/src/skills/`'s `envelope.ts`, `run-state.ts`,
// `grill-turn-state.ts`, `microtask-args.ts` and `grill-args.ts` are now
// thin wrappers over. Their exported signatures are unchanged, so every
// existing suite in that directory still tests the rule from this side.
//
// **The reducers cross as TEXT, not as objects.** A caller holds its state
// as a parsed object; the wrapper serializes it, asks the core, and — when
// the answer is byte-identical to what it sent — returns *the object it was
// given*, unparsed. That is not an optimisation: a reducer that always
// answered a fresh object would break the no-op rule the lane is built on
// (a duplicate tap must leave the running state untouched, and React
// re-renders on identity), so the identity is part of the contract, and it
// is the core's own byte-for-byte no-op that establishes it.
//
// **The three decline constants are NOT here.** `NO_TOKEN`,
// `NO_TERMINAL_LINE` and `OUTSIDE_SCHEMA` are read at module-evaluation
// time by `route-run.ts` and `useMicrotaskWiring.ts`, which are statically
// reachable from `main.tsx` — a seam call there would throw the "used
// before ready" guard on every page load. They stay literal TS strings in
// their own modules, pinned against the core by `seam.test.ts`, exactly as
// `priority.ts`'s `priorityRank` and `field-vocabulary.ts`'s arrays are.
// ADR-0025's #538 amendment records that as a verdict-table row rather than
// leaving it as folklore.

/** `hummingbird_core::decisions::skills::classify_line`, parsed. The object
 * shape is `envelope.ts`'s own `SkillLine`, which is why that module can
 * cast this and change nothing else. */
export function classifySkillLine(text: string): unknown {
  return JSON.parse(required().classify_skill_line(text));
}

/** `skills::microtask_result` / `skills::grill_result` over a terminal
 * line's `result` — `null` when the value is not that schema's shape.
 * `undefined` crosses as JSON `null`, which both readers answer `null` for
 * anyway. */
export function microtaskResultFromCore(result: unknown): unknown {
  return JSON.parse(required().microtask_result_json(JSON.stringify(result) ?? "null"));
}

export function grillResultFromCore(result: unknown): unknown {
  return JSON.parse(required().grill_result_json(JSON.stringify(result) ?? "null"));
}

/** `skills::reduce_run` / `skills::reduce_grill_turn`. The state crosses as
 * text; **the state the caller gave back is returned unchanged when the
 * core answered byte-identically**, so a no-op reduce is a no-op by
 * identity too. See the header for why that is contractual rather than an
 * optimisation. */
export function reduceSkillRun<S>(state: S, event: unknown): S {
  return reduceThroughCore(state, event, (s, e) => required().reduce_skill_run(s, e));
}

export function reduceGrillTurn<S>(state: S, event: unknown): S {
  return reduceThroughCore(state, event, (s, e) => required().reduce_grill_turn(s, e));
}

function reduceThroughCore<S>(
  state: S,
  event: unknown,
  reduce: (stateJson: string, eventJson: string) => string,
): S {
  const before = JSON.stringify(state);
  const after = reduce(before, JSON.stringify(event));
  return after === before ? state : (JSON.parse(after) as S);
}

/** `skills::stamp_label` — `null` whenever the envelope named no backend.
 * There is no default name to fall back to, here or in the core. */
export function skillStampLabel(state: unknown): string | null {
  return required().skill_stamp_label(JSON.stringify(state)) ?? null;
}

/** `skills::microtask_run_body` / `skills::grill_run_body` — the exact
 * request text, byte-pinned across Rust, TypeScript and Kotlin by
 * `client/core/tests/fixtures/skills-run-bodies.json`. */
export function microtaskRunBodyJson(input: unknown): string {
  return required().microtask_run_body_json(JSON.stringify(input));
}

export function grillRunBodyJson(ref: string, turns: unknown): string {
  return required().grill_run_body_json(ref, JSON.stringify(turns));
}

/** `skills::format_grill_transcript` — the plain-text record
 * `Core::complete_grill`'s `GrillCompletion.transcript` carries. */
export function formatGrillTranscriptFromCore(turns: unknown): string {
  return required().format_grill_transcript(JSON.stringify(turns));
}

/** `skills::decline_for_transport` / `skills::decline_for_response`. Both
 * are called from an event handler, past the loading gate — unlike the
 * three constants, which is the whole distinction the header draws. */
export function declineForTransportFromCore(detail: string): string {
  return required().decline_for_transport(detail);
}

export function declineForResponseFromCore(status: number): string {
  return required().decline_for_response(status);
}

/** The three module-evaluation-time constants, exposed for `seam.test.ts`
 * to pin the literal TS copies against — production reads the literals. */
export function noTokenDeclineFromCore(): string {
  return required().no_token_decline();
}

export function noTerminalLineDeclineFromCore(): string {
  return required().no_terminal_line_decline();
}

export function outsideSchemaDeclineFromCore(): string {
  return required().outside_schema_decline();
}

// -------------------------------------------------------------- M4 (#539)
// The microtask affordance (`skills/microtask-affordance.ts`), the backend
// picker's tier fallback and degrade-to-Auto rule
// (`skills/backend-registry.ts`'s `fallbackEntry`,
// `skills/backend-selection.ts`'s `readBackendSelection`), and the Grill
// review card's predicates (`screens/grill-review.ts`) — all sunk to
// `hummingbird_core::decisions::skills`.

/** The seven fields [`skills::affordance`]/`grill-review.ts`'s own
 * predicates read off a `StepDTO` — everything but `deletedAt`/`done` rides
 * along unread, the same "cross the whole DTO, read only what the rule
 * needs" shape [`backtestPayload`] already uses. Named once here rather
 * than repeated across every signature below that takes a step list. */
function stepsPayload(steps: readonly StepDTO[]): string {
  return JSON.stringify(steps);
}

/** `hummingbird_core::decisions::skills::microtask_affordance`, parsed —
 * the object shape is `microtask-affordance.ts`'s own `MicrotaskAffordance`. */
export function microtaskAffordanceFromCore(steps: readonly StepDTO[]): unknown {
  return JSON.parse(required().microtask_affordance_json(stepsPayload(steps)));
}

/** `hummingbird_core::decisions::skills::fallback_backend_id` — the next
 * registered id that is not `deadId`, `undefined` when there is none.
 * `registryIds` is a bare list of ids, the only part of a `BackendEntry`
 * this rule reads. */
export function fallbackBackendIdFromCore(registryIds: readonly string[], deadId: string): string | undefined {
  return required().fallback_backend_id(JSON.stringify(registryIds), deadId);
}

/** `hummingbird_core::decisions::skills::resolve_backend_selection` — Auto
 * when nothing is stored, or when the stored id no longer names a
 * registered entry. */
export function resolveBackendSelectionFromCore(stored: string | undefined, registryIds: readonly string[]): string {
  return required().resolve_backend_selection(stored, JSON.stringify(registryIds));
}

/** The sentinel selection value — module-evaluation-time in
 * `backend-registry.ts`'s own `AUTO_SELECTION`, pinned against this by
 * `seam.test.ts` for the same reason the three decline constants above
 * are. */
export function backendAutoSelectionFromCore(): string {
  return required().backend_auto_selection();
}

/** `hummingbird_core::decisions::skills::declined_backend_fallback` — #274's
 * one-tap fallback offer, decided whole: not declined / Auto / answered /
 * NO_TOKEN are all excluded inside this one call, never assembled from
 * three separate reads on this side (#539's round-2 review moved the
 * predicate out of `ffi-mobile` for the identical reason). `state` crosses
 * as the `SkillRunState` JSON `reduce_skill_run` already round-trips. */
export function declinedBackendFallbackFromCore(
  state: unknown,
  selection: string,
  registryIds: readonly string[],
): string | undefined {
  return required().declined_backend_fallback(JSON.stringify(state), selection, JSON.stringify(registryIds));
}

/** `hummingbird_core::decisions::skills::would_strand_plan`. */
export function grillWouldStrandPlanFromCore(verdict: string, steps: readonly StepDTO[]): boolean {
  return required().grill_would_strand_plan(verdict, stepsPayload(steps));
}

/** `hummingbird_core::decisions::skills::plan_replacement_label`. */
export function grillPlanReplacementLabelFromCore(steps: readonly StepDTO[]): string {
  return required().grill_plan_replacement_label(stepsPayload(steps));
}

/** `hummingbird_core::decisions::skills::demotes_from_frontier`. */
export function grillDemotesFromFrontierFromCore(verdict: string, stage: string): boolean {
  return required().grill_demotes_from_frontier(verdict, stage);
}

/** `hummingbird_core::decisions::skills::FRONTIER_DEMOTION_WARNING` —
 * pinning-test-only; `grill-review.ts`'s own copy stays a literal TS
 * string for the same module-evaluation-order reason the three decline
 * constants above do. */
export function grillFrontierDemotionWarningFromCore(): string {
  return required().grill_frontier_demotion_warning();
}

// -------------------------------------------------------------- M4 (#533)
// The standing-question panes: the pane shell contract's decided half
// (`AnswerState`, `Band`, `withinBand`, pane identity), the cross-pane
// sort, and the waste pane's whole rule set —
// `hummingbird_core::decisions::panes`.
//
// **The zone bridge is why two of these take a second argument.** Panes are
// civil-date reasoning and the core owns no tzdb, so a pane is answered in
// two phases: the core names the `(zone, civil-date)` facts it needs
// (`wasteZoneQueries`/`paneZoneQueries`), the host resolves them with
// `Intl` (`screens/questions/zone-bridge.ts` — this file deliberately does
// NOT import that module, so the seam stays a pure boundary with no screen
// dependency), and the core decides against the resolved table. A key the
// host could not resolve is simply **absent**, and what an absent fact
// means is a core decision like every other one.
//
// **What still crosses back as words:** nothing. Every wrapper below
// returns structured values — gap *kinds*, band names, indices, instants —
// and the sentences are composed client-side in `waste.ts`/
// `WastePaneExpanded.tsx`. That is ADR-0025's line: two clients disagreeing
// about the band is a bug, two clients wording a gap differently is a
// design choice.

/** One `(zone, civil-date)` fact the core cannot answer itself.
 * `key` is `ZoneQuery::key()`, sent by the core rather than derived here:
 * it is the whole protocol, and a second spelling of it on this side would
 * present as an unresolvable zone rather than as a bug. */
export type ZoneQuery =
  | { key: string; kind: "civilDate"; zone: string; atMs: number }
  | { key: string; kind: "midnight"; zone: string; date: string };

/** Everything the host could resolve, keyed by [`ZoneQuery.key`]. **An
 * omitted key is the unresolvable zone** — never a null, never an empty
 * string, because either would be this side deciding what an unusable zone
 * means. */
export type ZoneFacts = Record<string, string | number>;

/** The fields `hummingbird_core::decisions::panes::inputs::PaneInputs` reads
 * out of the shell's whole `QuestionInputs` — the same "do not re-cross
 * whole DTOs" discipline as `frontierPayload` above. Grew at #534 from
 * waste's original three (`nowMs`/`bindings`/`paneReads`) to also carry the
 * calendar arm, the item list and the sync snapshot, since weekend/vacation
 * read the former two and reachability reads the latter — each field added
 * only once a real sunk pane needed it. */
export interface PaneInputsSource {
  nowMs: number;
  bindings: BindingDTO[] | null;
  paneReads: Record<string, PaneReadDTO | undefined>;
  calendarReads?: Record<string, CalendarReadDTO | undefined>;
  calendarConnected?: boolean;
  items?: TaskItemDTO[];
  sync?: {
    latestOutcome: { kind: string } | null;
    latestInformativeAtMs: number | null;
    lastSuccessfulAtMs: number | null;
  };
  /** #715: which standing questions are switched off, as their wire
   * spellings. Optional and absent-means-none, `PaneInputs`'
   * `#[serde(default)]` on the other side of the same crossing. A question
   * in this list contributes no `ZoneQuery` and no ranked pane. */
  disabledQuestions?: string[];
}

function paneInputsPayload(inputs: PaneInputsSource): string {
  return JSON.stringify({
    nowMs: inputs.nowMs,
    bindings: inputs.bindings,
    paneReads: inputs.paneReads,
    calendarReads: inputs.calendarReads ?? {},
    calendarConnected: inputs.calendarConnected ?? false,
    items: inputs.items ?? [],
    sync: {
      latestOutcomeKind: inputs.sync?.latestOutcome?.kind ?? null,
      latestInformativeAtMs: inputs.sync?.latestInformativeAtMs ?? null,
      lastSuccessfulAtMs: inputs.sync?.lastSuccessfulAtMs ?? null,
    },
    disabledQuestions: inputs.disabledQuestions ?? [],
  });
}

/** `hummingbird_core::decisions::panes::waste::waste_zone_queries` — phase
 * one of the bridge for the waste pane alone. Empty when the payload is
 * already a gap: there is no zone to ask about. */
export function wasteZoneQueries(inputs: PaneInputsSource): ZoneQuery[] {
  return JSON.parse(required().waste_zone_queries_json(paneInputsPayload(inputs))) as ZoneQuery[];
}

/** `hummingbird_core::decisions::panes::zone_queries` — phase one for a
 * whole surface, deduplicated by key. */
export function paneZoneQueries(inputs: PaneInputsSource, surface: PaneSurface): ZoneQuery[] {
  return JSON.parse(
    required().pane_zone_queries_json(paneInputsPayload(inputs), surface),
  ) as ZoneQuery[];
}

/** `hummingbird_core::decisions::panes::alarm::status_alarm` — the Status
 * nav control's whole reading: the most salient band the Status surface
 * currently answers, or `undefined` when nothing there raises the nav.
 *
 * Takes no zone facts, unlike `rankPanes` beside it — none of the status
 * four is civil-date reasoning, so there is nothing for this host to
 * resolve first (the core asserts that rather than assuming it). `null` on
 * the wire becomes `undefined` here, so a caller spreads it into props
 * without a second falsy value to think about. */
export function statusAlarm(inputs: PaneInputsSource): PaneBand | undefined {
  const band = JSON.parse(required().status_alarm_json(paneInputsPayload(inputs))) as
    | PaneBand
    | null;
  return band ?? undefined;
}

export type PaneSurface = "now" | "status";

/** `hummingbird_core::decisions::panes::contract::AnswerState`. */
export type PaneAnswerState = "answered" | "bound-but-unacquired" | "unbound";

/** `hummingbird_core::decisions::panes::contract::Band`. */
export type PaneBand = "live" | "imminent" | "near" | "distant" | "dormant";

/** `PaneAnswer` minus its rendering half — the three fields ADR-0025 sinks.
 * The headline and the glyphs stay per-client and are added back by the
 * pane's own module. */
export interface PaneAnswerCore {
  answerState: PaneAnswerState;
  band: PaneBand;
  withinBand: number | null;
}

/** One pane as the sort reads it — structurally what `RankedPane`
 * (`screens/questions/contract.ts`) already is, named here so this file
 * needs no import from `screens/`. */
export interface RankedPaneLike {
  question: string;
  subjectKey: string;
  paneKey: string;
  answer: PaneAnswerCore;
}

/** `hummingbird_core::decisions::panes::sort::order_panes`.
 *
 * Generic over the caller's own pane type, and mapped back by **index**
 * rather than by `paneKey`: the wasm side returns ordered input positions
 * (see `order_panes_json`'s own doc for why identity would be the wrong
 * key here), so a caller keeps whatever headline and glyphs its panes
 * carry. */
export function orderPanes<T extends RankedPaneLike>(
  panes: readonly T[],
  questionOrder: readonly string[],
): T[] {
  const indices = JSON.parse(
    required().order_panes_json(panePayload(panes), JSON.stringify(questionOrder)),
  ) as number[];
  return indices.map((index) => panes[index]);
}

/** `hummingbird_core::decisions::panes::sort::same_pane_identity` —
 * deliberately not a full equality; see the core function's own doc. */
export function samePaneIdentity(
  a: readonly RankedPaneLike[],
  b: readonly RankedPaneLike[],
): boolean {
  return required().same_pane_identity_json(panePayload(a), panePayload(b));
}

/** The four fields the sort touches, and nothing the shell draws with. */
function panePayload(panes: readonly RankedPaneLike[]): string {
  return JSON.stringify(
    panes.map((pane) => ({
      question: pane.question,
      subjectKey: pane.subjectKey,
      paneKey: pane.paneKey,
      answer: {
        answerState: pane.answer.answerState,
        band: pane.answer.band,
        withinBand: pane.answer.withinBand,
      },
    })),
  );
}

// `hummingbird_core::decisions::panes::rank_panes` — the batched,
// surface-level ranking path — is deliberately NOT wrapped here. #533 left
// it as an open ergonomics question ("whether the web should hoist onto
// it"); #534's answer is no: every pane's `answer`/`subjects` sank
// individually, but `registry.ts`'s `panesFrom` still ranks per-question,
// exactly as it did before this slice (hoisting would mean rewriting
// `contract.ts`'s `QuestionDef`, `registry.ts` and `RankedRegion.tsx` for
// no behavioural gain — the per-question path already produces the same
// ranked, sorted result `rank_panes` would). The core's own
// `rank_panes`/`zone_queries` stay exercised by `client/core`'s and
// `hummingbird-ffi-web`'s own test suites; this file no longer carries a
// second, uncalled copy of the wrapper (the finding #534's own issue
// recorded: an export with no production caller is exactly what
// TypeScript's checker cannot flag on its own).

/** `hummingbird_core::decisions::panes::waste::Stream` — kerb vocabulary. */
export type WasteStream = "trash" | "recycling" | "yard";

/** `hummingbird_core::decisions::panes::waste::WasteGap` — a **kind**, not
 * a sentence. `waste.ts`'s `wasteGapReason` is the one place these become
 * words. `malformed`'s `reason` is the domain's own wording
 * (`EnvelopeProblem`) passed through as data, not composed by the core. */
export type WasteGap =
  | { gap: "notFetched" }
  | { gap: "malformed"; reason: string }
  | { gap: "unknownSchema"; schema: string }
  | { gap: "notJson" }
  | { gap: "notAnObject" }
  | { gap: "noZone" }
  | { gap: "badDates" }
  | { gap: "unknownStream" }
  | { gap: "unresolvableZone"; zone: string }
  | { gap: "pastCollection"; collectedOn: string; weekdayIndex: number };

/** `hummingbird_core::decisions::panes::waste::WasteFacts` — everything an
 * answered waste pane needs, with no rendered sentence in it.
 * `weekdayIndex` is `0` = Sunday; the *word* is per-client. */
export interface WasteFacts {
  zone: string;
  scheduled: string;
  collectedOn: string;
  streams: WasteStream[];
  today: string;
  daysAway: number;
  holiday: boolean;
  weekdayIndex: number;
  stale: boolean;
  startsAtMs: number;
  freshness: FreshnessDTO;
}

export type WasteResolved = ({ kind: "facts" } & WasteFacts) | { kind: "gap"; gap: WasteGap };

/** `hummingbird_core::decisions::panes::waste::waste_facts` — phase two for
 * the waste pane alone. */
export function wasteFactsFromCore(inputs: PaneInputsSource, facts: ZoneFacts): WasteResolved {
  return JSON.parse(
    required().waste_facts_json(paneInputsPayload(inputs), JSON.stringify(facts)),
  ) as WasteResolved;
}

/** `hummingbird_core::decisions::panes::waste::waste_answer`. */
export function wasteAnswerFromCore(
  inputs: PaneInputsSource,
  facts: ZoneFacts,
): PaneAnswerCore {
  return JSON.parse(
    required().waste_answer_json(paneInputsPayload(inputs), JSON.stringify(facts)),
  ) as PaneAnswerCore;
}

/** `hummingbird_core::decisions::panes::waste::WasteSetup` — four answers,
 * not a boolean. */
export type WasteSetupCore =
  | { kind: "bound"; page: string }
  | { kind: "unread" }
  | { kind: "unusable" }
  | { kind: "unset" };

/** `hummingbird_core::decisions::panes::waste::waste_setup`. */
export function wasteSetupFromCore(inputs: PaneInputsSource): WasteSetupCore {
  return JSON.parse(required().waste_setup_json(paneInputsPayload(inputs))) as WasteSetupCore;
}

export interface WasteBodyCore {
  zone: string;
  scheduled: string;
  collectedOn: string;
  streams: WasteStream[];
}

/** `hummingbird_core::decisions::panes::waste::parse_waste_body` — shape
 * only. A zone this runtime cannot resolve is not a shape problem and is
 * refused later, by the core, when the bridge comes back without the
 * fact. */
export function parseWasteBodyFromCore(
  snapshot: PaneSnapshotDTO | undefined,
): { kind: "ok"; body: WasteBodyCore } | { kind: "gap"; gap: WasteGap } {
  return JSON.parse(required().parse_waste_body_json(JSON.stringify(snapshot ?? null))) as
    | { kind: "ok"; body: WasteBodyCore }
    | { kind: "gap"; gap: WasteGap };
}

/** `hummingbird_core::decisions::panes::BAND_ORDER` — pinning-test-only.
 * `contract.ts`'s own `BAND_ORDER` is a module-evaluation-time literal
 * (`registry.ts` builds `QUESTIONS` at module evaluation and would throw
 * this file's "used before ready" guard on every page load), and
 * `seam.test.ts` pins the two together. */
export function paneBandOrderFromCore(): PaneBand[] {
  return JSON.parse(required().pane_band_order_json()) as PaneBand[];
}

/** `hummingbird_core::decisions::panes::QUESTION_ORDER` — pinning-test-only
 * for the same reason. */
export function paneQuestionOrderFromCore(): string[] {
  return JSON.parse(required().pane_question_order_json()) as string[];
}

export interface WasteConstants {
  source: string;
  snapshotKey: string;
  bindingKey: string;
  staleAfterMs: number;
  streamOrder: WasteStream[];
}

/** The waste pane's constants — pinning-test-only, same reason again:
 * `waste.ts`'s `SOURCE` is read at module evaluation by `question.ts`'s
 * `sources` array. */
export function wasteConstantsFromCore(): WasteConstants {
  return JSON.parse(required().waste_constants_json()) as WasteConstants;
}

// -------------------------------------------------------------- #535 (M4)
// The Settings screen's decision half: `hummingbird_core::decisions::
// settings` — the sync-status readout and the dead-letter heading, sunk so
// Android does not carry its own copy of what "stale"/"held"/"synced"
// mean. `shell/sync-status.ts` is the one caller; everything else in this
// app reaches these through that module, never this one directly.

/** `hummingbird_core::decisions::settings::SyncStatusInput`, camelCase to
 * match the JSON the wasm seam reads. */
export interface SyncStatusInputCore {
  online: boolean;
  lastSyncOutcomeKind: string | null;
  lastSyncAtMs: number | null;
  queueDepth: number | null;
  nowMs: number;
}

/** `hummingbird_core::decisions::settings::SyncStatusTone`. */
export type SyncStatusToneCore = "neutral" | "warn" | "danger" | "success";

export interface SyncStatusSummaryCore {
  tone: SyncStatusToneCore;
  label: string;
  toneWord: string;
}

/** `hummingbird_core::decisions::settings::{sync_status_tone,
 * sync_status_label, sync_status_tone_word}`, answered together off one
 * input so the badge and its label can never disagree about which state
 * they describe. */
export function syncStatusSummaryFromCore(input: SyncStatusInputCore): SyncStatusSummaryCore {
  return JSON.parse(required().sync_status_summary_json(JSON.stringify(input))) as SyncStatusSummaryCore;
}

/** `hummingbird_core::decisions::settings::dead_letter_heading`. */
export function deadLetterHeadingFromCore(count: number): string {
  return required().dead_letter_heading(count);
}

/** `hummingbird_core::decisions::settings::SyncOutcomeClass`'s wire
 * spelling. */
export type SyncOutcomeClassCore = "held" | "failed" | "not-run" | "landed";

/** `hummingbird_core::decisions::settings::sync_outcome_class`. */
export function syncOutcomeClassFromCore(kind: string): SyncOutcomeClassCore {
  return required().sync_outcome_class(kind) as SyncOutcomeClassCore;
}

/** `hummingbird_core::decisions::settings::is_informative_sync_outcome`. */
export function isInformativeSyncOutcomeFromCore(kind: string): boolean {
  return required().is_informative_sync_outcome(kind);
}

/** `hummingbird_core::decisions::settings::relative_age`. */
export function relativeAgeFromCore(ageMs: number): string {
  return required().relative_age(ageMs);
}

// ------------------------------------------------------------------- #534
// The remaining seven panes: the status four (kimi/github/uptime/
// reachability) and the now three (race/weekend/vacation). Same house style
// as waste's own section: structured values only, never a rendered
// sentence — each pane's own module composes its words from these.

function snapshotPayload(snapshot: PaneSnapshotDTO | undefined): string {
  return JSON.stringify(snapshot ?? null);
}

// -- kimi (#313) -------------------------------------------------------

export type KimiGap =
  | { gap: "notFetched" }
  | { gap: "malformed"; reason: string }
  | { gap: "unknownSchema"; schema: string }
  | { gap: "notJson" }
  | { gap: "notAnObject" }
  | { gap: "badNumbers" };

export interface KimiBodyCore {
  availableBalance: number;
  voucherBalance: number;
  cashBalance: number;
}

export interface KimiFacts {
  availableBalance: number;
  voucherBalance: number;
  cashBalance: number;
  stale: boolean;
  freshness: FreshnessDTO;
}

export type KimiResolved = ({ kind: "facts" } & KimiFacts) | { kind: "gap"; gap: KimiGap };

export function parseKimiBodyFromCore(
  snapshot: PaneSnapshotDTO | undefined,
): { kind: "ok"; body: KimiBodyCore } | { kind: "gap"; gap: KimiGap } {
  return JSON.parse(required().parse_kimi_body_json(snapshotPayload(snapshot))) as
    | { kind: "ok"; body: KimiBodyCore }
    | { kind: "gap"; gap: KimiGap };
}

export function kimiFactsFromCore(inputs: PaneInputsSource): KimiResolved {
  return JSON.parse(required().kimi_facts_json(paneInputsPayload(inputs))) as KimiResolved;
}

export function kimiAnswerFromCore(inputs: PaneInputsSource): PaneAnswerCore {
  return JSON.parse(required().kimi_answer_json(paneInputsPayload(inputs))) as PaneAnswerCore;
}

export function kimiBandFromCore(availableBalance: number): PaneBand {
  return JSON.parse(required().kimi_band_json(availableBalance)) as PaneBand;
}

export interface KimiConstants {
  source: string;
  snapshotKey: string;
  staleAfterMs: number;
  imminentThresholdUsd: number;
  nearThresholdUsd: number;
}

export function kimiConstantsFromCore(): KimiConstants {
  return JSON.parse(required().kimi_constants_json()) as KimiConstants;
}

// -- github (#314) -------------------------------------------------------

export type WorkflowGap =
  | { gap: "notFetched" }
  | { gap: "malformed"; reason: string }
  | { gap: "unknownSchema"; schema: string }
  | { gap: "notJson" }
  | { gap: "notAnObject" }
  | { gap: "unreadableFields" };

export interface WorkflowBodyCore {
  displayName: string;
  declaredCadenceMs: number | null;
  lastRunConclusion: string | null;
  lastRunEvent: string | null;
  lastRunAtMs: number | null;
  lastScheduledSuccessAtMs: number | null;
}

export interface WorkflowFacts {
  body: WorkflowBodyCore;
  stale: boolean;
  freshness: FreshnessDTO;
}

export type WorkflowResolved =
  | { kind: "view" } & WorkflowFacts
  | { kind: "gap"; gap: WorkflowGap };

export function parseWorkflowBodyFromCore(
  snapshot: PaneSnapshotDTO | undefined,
): { kind: "ok"; body: WorkflowBodyCore } | { kind: "gap"; gap: WorkflowGap } {
  return JSON.parse(required().parse_workflow_body_json(snapshotPayload(snapshot))) as
    | { kind: "ok"; body: WorkflowBodyCore }
    | { kind: "gap"; gap: WorkflowGap };
}

/** When the poller *observed* what a row reports — `github.rs`'s
 * `observed_at_ms`, the only thing that may build `githubBandFromCore`'s
 * second argument. `null` when the row's freshness cannot locate it. */
export function githubObservedAtMsFromCore(nowMs: number, freshness: FreshnessDTO): number | null {
  return JSON.parse(required().github_observed_at_ms_json(nowMs, JSON.stringify(freshness))) as number | null;
}

/** `observedAtMs` is when the *poller* looked — never `nowMs`. See
 * `github.rs`'s `observed_at_ms`: judging a workflow overdue against the
 * reader's own clock is what made every short-cadence workflow read "cron
 * stalled" between polls. `null` means the row's freshness could not locate
 * the observation at all. */
export function githubBandFromCore(body: WorkflowBodyCore, observedAtMs: number | null): PaneBand {
  return JSON.parse(required().github_band_json(JSON.stringify(body), observedAtMs ?? undefined)) as PaneBand;
}

export function githubSubjectsFromCore(inputs: PaneInputsSource): string[] {
  return JSON.parse(required().github_subjects_json(paneInputsPayload(inputs))) as string[];
}

export function githubFactsFromCore(subjectKey: string, inputs: PaneInputsSource): WorkflowResolved {
  return JSON.parse(
    required().github_facts_json(subjectKey, paneInputsPayload(inputs)),
  ) as WorkflowResolved;
}

export function githubAnswerFromCore(subjectKey: string, inputs: PaneInputsSource): PaneAnswerCore {
  return JSON.parse(
    required().github_answer_json(subjectKey, paneInputsPayload(inputs)),
  ) as PaneAnswerCore;
}

export interface GithubConstants {
  source: string;
  neverPolledSubject: string;
  staleAfterMs: number;
  overdueMultiplier: number;
  minOverdueAfterMs: number;
}

export function githubConstantsFromCore(): GithubConstants {
  return JSON.parse(required().github_constants_json()) as GithubConstants;
}

// -- uptime (#315) -------------------------------------------------------

export type UptimeExpected = "on" | "off";

export type ProbeGap =
  | { gap: "notFetched" }
  | { gap: "malformed"; reason: string }
  | { gap: "unknownSchema"; schema: string }
  | { gap: "notJson" }
  | { gap: "notAnObject" }
  | { gap: "fieldsUnreadable" }
  | { gap: "observationUnreadable" };

export interface ProbeBodyCore {
  expected: UptimeExpected;
  expectStatus: number;
  observedStatus: number | null;
  error: string | null;
}

export interface ProbeFacts {
  serviceId: string;
  body: ProbeBodyCore;
  stale: boolean;
  freshness: FreshnessDTO;
}

export type ProbeResolved = ({ kind: "facts" } & ProbeFacts) | { kind: "gap"; gap: ProbeGap };

export function parseUptimeBodyFromCore(
  snapshot: PaneSnapshotDTO | undefined,
): { kind: "ok"; body: ProbeBodyCore } | { kind: "gap"; gap: ProbeGap } {
  return JSON.parse(required().parse_uptime_body_json(snapshotPayload(snapshot))) as
    | { kind: "ok"; body: ProbeBodyCore }
    | { kind: "gap"; gap: ProbeGap };
}

export function uptimeBandFromCore(body: ProbeBodyCore): PaneBand {
  return JSON.parse(required().uptime_band_json(JSON.stringify(body))) as PaneBand;
}

export function uptimeSubjectsFromCore(inputs: PaneInputsSource): string[] {
  return JSON.parse(required().uptime_subjects_json(paneInputsPayload(inputs))) as string[];
}

export function uptimeFactsFromCore(subjectKey: string, inputs: PaneInputsSource): ProbeResolved {
  return JSON.parse(
    required().uptime_facts_json(subjectKey, paneInputsPayload(inputs)),
  ) as ProbeResolved;
}

export function uptimeAnswerFromCore(subjectKey: string, inputs: PaneInputsSource): PaneAnswerCore {
  return JSON.parse(
    required().uptime_answer_json(subjectKey, paneInputsPayload(inputs)),
  ) as PaneAnswerCore;
}

export interface UptimeConstants {
  source: string;
  neverPolledSubject: string;
  staleAfterMs: number;
}

export function uptimeConstantsFromCore(): UptimeConstants {
  return JSON.parse(required().uptime_constants_json()) as UptimeConstants;
}

// -- reachability (#316) --------------------------------------------------

export interface ReachabilityFacts {
  ageMs: number;
  stale: boolean;
  latestAttemptLanded: boolean;
}

export function reachabilityFactsFromCore(inputs: PaneInputsSource): ReachabilityFacts | null {
  return JSON.parse(required().reachability_facts_json(paneInputsPayload(inputs))) as
    | ReachabilityFacts
    | null;
}

export function reachabilityAnswerFromCore(inputs: PaneInputsSource): PaneAnswerCore {
  return JSON.parse(required().reachability_answer_json(paneInputsPayload(inputs))) as PaneAnswerCore;
}

export interface ReachabilityConstants {
  subjectKey: string;
  graceMs: number;
}

export function reachabilityConstantsFromCore(): ReachabilityConstants {
  return JSON.parse(required().reachability_constants_json()) as ReachabilityConstants;
}

// -- race (#119) -----------------------------------------------------------

export interface RaceSessionCore {
  kind: string;
  label: string;
  startsAtMs: number;
}

export interface RaceEventCore {
  name: string;
  locality: string;
  startsAtMs: number;
  sessions: RaceSessionCore[];
}

export interface RaceBodyCore {
  events: RaceEventCore[];
}

export type RaceGap =
  | { gap: "notFetched" }
  | { gap: "malformed"; reason: string }
  | { gap: "unknownSchema"; schema: string }
  | { gap: "notJson" }
  | { gap: "notAnObject" }
  | { gap: "noSeason" }
  | { gap: "badEvent" };

export type RaceSetupCore =
  | { kind: "bound"; series: string[] }
  | { kind: "unread" }
  | { kind: "unusable" }
  | { kind: "unset" };

export interface RaceFacts {
  series: string;
  event: RaceEventCore | null;
  nextStart: [string, number] | null;
  hasLiveAlert: boolean;
  stale: boolean;
  freshness: FreshnessDTO;
}

export type RaceResolved = ({ kind: "facts" } & RaceFacts) | { kind: "gap"; gap: RaceGap };

export function parseRaceBodyFromCore(
  snapshot: PaneSnapshotDTO | undefined,
): { kind: "ok"; body: RaceBodyCore } | { kind: "gap"; gap: RaceGap } {
  return JSON.parse(required().parse_race_body_json(snapshotPayload(snapshot))) as
    | { kind: "ok"; body: RaceBodyCore }
    | { kind: "gap"; gap: RaceGap };
}

export function raceSeriesFromBindingFromCore(text: string): string[] {
  return JSON.parse(required().race_series_from_binding_json(text)) as string[];
}

export function raceSetupFromCore(inputs: PaneInputsSource): RaceSetupCore {
  return JSON.parse(required().race_setup_json(paneInputsPayload(inputs))) as RaceSetupCore;
}

export function raceSubjectsFromCore(inputs: PaneInputsSource): string[] {
  return JSON.parse(required().race_subjects_json(paneInputsPayload(inputs))) as string[];
}

export function nextRaceAtFromCore(events: RaceEventCore[], nowMs: number): RaceEventCore | null {
  return JSON.parse(required().next_race_at_json(JSON.stringify(events), nowMs)) as RaceEventCore | null;
}

export function raceFactsFromCore(series: string, inputs: PaneInputsSource): RaceResolved {
  return JSON.parse(required().race_facts_json(series, paneInputsPayload(inputs))) as RaceResolved;
}

export function raceAnswerFromCore(subjectKey: string, inputs: PaneInputsSource): PaneAnswerCore {
  return JSON.parse(
    required().race_answer_json(subjectKey, paneInputsPayload(inputs)),
  ) as PaneAnswerCore;
}

export interface RaceConstants {
  source: string;
  bindingKey: string;
  staleAfterMs: number;
  setupSubject: string;
}

export function raceConstantsFromCore(): RaceConstants {
  return JSON.parse(required().race_constants_json()) as RaceConstants;
}

// -- homework (#675) --------------------------------------------------------

/** One open homework item — `homework.rs`'s `HomeworkItem`. `description`
 * is carried on the winner alone (see that module's own doc); on an entry
 * in `others` it is always `null`. */
export interface HomeworkItemCore {
  id: string;
  title: string;
  deadline: string | null;
  description: string | null;
}

export interface HomeworkFactsCore {
  winner: HomeworkItemCore | null;
  others: HomeworkItemCore[];
  /** Whole civil days from the device's today to the winner's deadline —
   * negative when overdue, `0` today, `null` when there is no winner or the
   * winner carries no deadline. **The number each client writes its own
   * sentence from**: ADR-0025 crosses facts, never sentences, and the
   * arithmetic behind this one cannot be redone client-side without the
   * zone bridge anyway. */
  daysAway: number | null;
}

export type HomeworkGap = "unresolvableZone";

export type HomeworkResolvedCore =
  | ({ kind: "facts" } & HomeworkFactsCore)
  | { kind: "gap"; gap: HomeworkGap };

/** `hummingbird_core::decisions::panes::homework::homework_zone_queries`.
 *
 * Takes the whole inputs rather than `nowMs` (which is what
 * `weekendZoneQueriesFromCore` takes): this pane names its midnight queries
 * from the open items' own deadlines, so there is nothing to over-ask and
 * nothing a caller could compute from the clock alone. */
export function homeworkZoneQueriesFromCore(inputs: PaneInputsSource): ZoneQuery[] {
  return JSON.parse(required().homework_zone_queries_json(paneInputsPayload(inputs))) as ZoneQuery[];
}

/** `hummingbird_core::decisions::panes::homework::homework_facts`. */
export function homeworkFactsFromCore(
  inputs: PaneInputsSource,
  facts: ZoneFacts,
): HomeworkResolvedCore {
  return JSON.parse(
    required().homework_facts_json(paneInputsPayload(inputs), JSON.stringify(facts)),
  ) as HomeworkResolvedCore;
}

/** `hummingbird_core::decisions::panes::homework::homework_answer`. */
export function homeworkAnswerFromCore(
  inputs: PaneInputsSource,
  facts: ZoneFacts,
): PaneAnswerCore {
  return JSON.parse(
    required().homework_answer_json(paneInputsPayload(inputs), JSON.stringify(facts)),
  ) as PaneAnswerCore;
}

/** `hummingbird_core::decisions::panes::homework::homework_link` — the
 * standing session link, or `null` when nothing usable is bound.
 *
 * Its own door rather than a field on the facts: the link is *standing*, so
 * it has to answer in the zone-gap arm too, and that arm carries no facts.
 * The `http`/`https` filter behind it is the core's — this value goes
 * straight to `window.open`, and deciding the scheme twice (once per
 * client) is exactly what ADR-0025 puts in the core instead. */
export function homeworkLinkFromCore(inputs: PaneInputsSource): string | null {
  return JSON.parse(required().homework_link_json(paneInputsPayload(inputs))) as string | null;
}

export interface HomeworkConstants {
  context: string;
  subjectKey: string;
  nearWithinDays: number;
  /** The `settings` key the standing session link is held under. */
  linkBindingKey: string;
}

export function homeworkConstantsFromCore(): HomeworkConstants {
  return JSON.parse(required().homework_constants_json()) as HomeworkConstants;
}

// -- weekend (#122) ---------------------------------------------------------

export interface WeekendDayCore {
  date: string;
  startMs: number;
  endMs: number;
}

export interface WeekendWindowCore {
  startMs: number;
  endMs: number;
  days: WeekendDayCore[];
  underWay: boolean;
}

export type WeekendGap = "notConnected" | "unacquired" | "unresolvableZone";

export interface WeekendCountsCore {
  events: number;
  due: number;
  scheduled: number;
}

/** `weekend.rs`'s `EntryKind` — the three things a merged entry is. */
export type WeekendEntryKind = "event" | "due" | "scheduled";

/** `weekend.rs`'s `EntryAnchor` — an instant within the day, or the whole
 * day. */
export type WeekendEntryAnchor = "time" | "day";

/** `weekend.rs`'s `WindowEntry` (#564). Note what it does NOT carry: the
 * event or the item. `sourceId` is the handle a host uses to reach back
 * into its own DTOs — `inputs.rs`'s "do not re-cross whole DTOs" rule,
 * applied on the way out. */
export interface WeekendEntryCore {
  id: string;
  kind: WeekendEntryKind;
  title: string;
  atMs: number;
  anchor: WeekendEntryAnchor;
  dayKey: string;
  sourceId: string;
  alsoScheduledOn: string | null;
  deadlineOutsideWindow: string | null;
}

export interface WeekendDayEntriesCore {
  date: string;
  entries: WeekendEntryCore[];
}

export interface WeekendFactsCore {
  window: WeekendWindowCore;
  counts: WeekendCountsCore;
  /** One per day still ahead, in window order — `window.days` verbatim,
   * so this shrinks as the weekend is spent. Never empty. */
  days: WeekendDayEntriesCore[];
}

export type WeekendResolvedCore =
  | ({ kind: "facts" } & WeekendFactsCore)
  | { kind: "gap"; gap: WeekendGap };

export function weekendZoneQueriesFromCore(nowMs: number): ZoneQuery[] {
  return JSON.parse(required().weekend_zone_queries_json(nowMs)) as ZoneQuery[];
}

export function weekendWindowFromCore(nowMs: number, facts: ZoneFacts): WeekendWindowCore | null {
  return JSON.parse(
    required().weekend_window_json(nowMs, JSON.stringify(facts)),
  ) as WeekendWindowCore | null;
}

export function weekendFactsFromCore(inputs: PaneInputsSource, facts: ZoneFacts): WeekendResolvedCore {
  return JSON.parse(
    required().weekend_facts_json(paneInputsPayload(inputs), JSON.stringify(facts)),
  ) as WeekendResolvedCore;
}

export function weekendAnswerFromCore(
  inputs: PaneInputsSource,
  facts: ZoneFacts,
): PaneAnswerCore {
  return JSON.parse(
    required().weekend_answer_json(paneInputsPayload(inputs), JSON.stringify(facts)),
  ) as PaneAnswerCore;
}

/** `hummingbird_core::decisions::panes::weekend::weekend_band` — exposed
 * standalone so `weekend.ts`'s locally-kept `weekendBand` can be pinned
 * directly against it, on `githubBandFromCore`'s own precedent. */
export function weekendBandFromCore(window: WeekendWindowCore, nowMs: number): PaneBand {
  return JSON.parse(required().weekend_band_json(JSON.stringify(window), nowMs)) as PaneBand;
}

/** `hummingbird_core::decisions::panes::weekend::weekend_within_band`. */
export function weekendWithinBandFromCore(window: WeekendWindowCore): number {
  return JSON.parse(required().weekend_within_band_json(JSON.stringify(window))) as number;
}

export interface WeekendConstants {
  subjectKey: string;
  calendarRequestKey: string;
  imminentWithinMs: number;
  nearWithinMs: number;
}

export function weekendConstantsFromCore(): WeekendConstants {
  return JSON.parse(required().weekend_constants_json()) as WeekendConstants;
}

// -- vacation (#121) ---------------------------------------------------------

export type TripPhaseCore = "upcoming" | "departs_today" | "under_way" | "returns_today" | "past";

export interface TripCore {
  id: string;
  location: string | null;
  startDate: string;
  lastDate: string;
  startMs: number;
  endMs: number;
  phase: TripPhaseCore;
  daysUntil: number;
  lengthDays: number;
  dayOfTrip: number;
}

export function vacationZoneQueriesFromCore(inputs: PaneInputsSource): ZoneQuery[] {
  return JSON.parse(
    required().vacation_zone_queries_json(paneInputsPayload(inputs)),
  ) as ZoneQuery[];
}

/** `hummingbird_core::decisions::panes::vacation::VacationSetupKind` — the
 * kind-only projection of `VacationSetup` (which cannot itself cross the
 * seam; its `Bound` arm borrows). `Bound` carries only `calendarId`; the
 * caller already has the bound read's events/freshness on its own
 * `QuestionInputs`. */
export type VacationSetupKindCore =
  | { kind: "noCalendar" }
  | { kind: "unbound" }
  | { kind: "unread" }
  | { kind: "bound"; calendarId: string };

/** `hummingbird_core::decisions::panes::vacation::vacation_setup_kind`. */
export function vacationSetupFromCore(inputs: PaneInputsSource): VacationSetupKindCore {
  return JSON.parse(required().vacation_setup_json(paneInputsPayload(inputs))) as VacationSetupKindCore;
}

export function tripQueueFromCore(
  events: CalendarEventDTO[],
  calendarId: string,
  today: string,
  facts: ZoneFacts,
): TripCore[] {
  return JSON.parse(
    required().trip_queue_json(JSON.stringify(events), calendarId, today, JSON.stringify(facts)),
  ) as TripCore[];
}

export function vacationBandFromCore(next: TripCore | null): PaneBand {
  return JSON.parse(required().vacation_band_json(JSON.stringify(next))) as PaneBand;
}

export type VacationGap = { gap: "unresolvableZone" };

export interface VacationFacts {
  next: TripCore | null;
  later: TripCore[];
  freshness: FreshnessDTO;
  stale: boolean;
}

export type VacationResolved =
  | ({ kind: "facts" } & VacationFacts)
  | { kind: "gap"; gap: VacationGap };

export function vacationViewFromCore(
  inputs: PaneInputsSource,
  facts: ZoneFacts,
): VacationResolved | null {
  return JSON.parse(
    required().vacation_view_json(paneInputsPayload(inputs), JSON.stringify(facts)),
  ) as VacationResolved | null;
}

export function vacationAnswerFromCore(
  inputs: PaneInputsSource,
  facts: ZoneFacts,
): PaneAnswerCore {
  return JSON.parse(
    required().vacation_answer_json(paneInputsPayload(inputs), JSON.stringify(facts)),
  ) as PaneAnswerCore;
}

export interface VacationConstants {
  subjectKey: string;
  calendarRequestKey: string;
  horizonBeforeDays: number;
  horizonAheadDays: number;
  staleAfterMs: number;
  imminentWithinDays: number;
  nearWithinDays: number;
}

export function vacationConstantsFromCore(): VacationConstants {
  return JSON.parse(required().vacation_constants_json()) as VacationConstants;
}

// -- scps (#693, ADR-0032) ---------------------------------------------------

export type ScpsKindCore = "meeting" | "activity" | "happy_hour" | "event";

export interface ScpsEventCore {
  id: string;
  kind: ScpsKindCore;
  topic: string | null;
  startMs: number;
  endMs: number;
  startDate: string;
  location: string | null;
  notes: string | null;
  daysUntil: number;
  inProgress: boolean;
}

export type ScpsQuestFactCore =
  | { kind: "none" }
  | { kind: "current"; phrase: string }
  | { kind: "other"; month: string; phrase: string };

export function scpsZoneQueriesFromCore(inputs: PaneInputsSource): ZoneQuery[] {
  return JSON.parse(required().scps_zone_queries_json(paneInputsPayload(inputs))) as ZoneQuery[];
}

export type ScpsGap = { gap: "unresolvableZone" };

export interface ScpsFacts {
  next: ScpsEventCore | null;
  later: ScpsEventCore[];
  quest: ScpsQuestFactCore;
  freshness: FreshnessDTO;
  stale: boolean;
}

export type ScpsResolved = ({ kind: "facts" } & ScpsFacts) | { kind: "gap"; gap: ScpsGap };

export function scpsViewFromCore(inputs: PaneInputsSource, facts: ZoneFacts): ScpsResolved | null {
  return JSON.parse(
    required().scps_view_json(paneInputsPayload(inputs), JSON.stringify(facts)),
  ) as ScpsResolved | null;
}

export function scpsAnswerFromCore(inputs: PaneInputsSource, facts: ZoneFacts): PaneAnswerCore {
  return JSON.parse(
    required().scps_answer_json(paneInputsPayload(inputs), JSON.stringify(facts)),
  ) as PaneAnswerCore;
}

export interface ScpsConstants {
  subjectKey: string;
  calendarRequestKey: string;
  questBindingKey: string;
  horizonBeforeMs: number;
  horizonAfterDays: number;
  staleAfterMs: number;
}

export function scpsConstantsFromCore(): ScpsConstants {
  return JSON.parse(required().scps_constants_json()) as ScpsConstants;
}

/** `hummingbird_core::decisions::panes::zone::DEVICE_ZONE` — the sentinel
 * `zone-bridge.ts`'s literal copy is pinned against, by `seam.test.ts`. */
export function deviceZoneFromCore(): string {
  return required().device_zone();
}

// -- the standing-question roster (#714) ------------------------------------

/** One standing question as the core lists it (ADR-0034 decision 4) — the
 * shape `question_roster_json` returns, and the only place the
 * question→binding relation is spelled. */
export interface QuestionRosterEntry {
  /** The wire spelling of `StandingQuestion` — kebab-case, and the same
   * string a `RankedPane.question` carries. */
  question: string;
  /** The operator-facing name. Canonical: no client declares its own. */
  label: string;
  /** ADR-0017's surface axis, straight out of the core's `SUNK`. */
  surface: string;
  /** The `settings` keys that answer this question — `[]` for most. */
  bindings: string[];
}

/** Every standing question, in `QUESTION_ORDER`. Applied result: the core
 * decides which questions exist, this client only draws them. */
export function questionRosterFromCore(): QuestionRosterEntry[] {
  return JSON.parse(required().question_roster_json()) as QuestionRosterEntry[];
}
