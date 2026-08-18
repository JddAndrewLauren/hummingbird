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
  FreshnessDTO,
  PaneReadDTO,
  PaneSnapshotDTO,
  ProjectDTO,
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
  // M1-4 (#502): the item stage-transition rules.
  item_available_actions(stage: string): string;
  item_applied_stage(action: string): string | undefined;
  item_can_mark_done(stage: string, archived: boolean): boolean;
  item_can_grill(stage: string): boolean;
  item_grill_button_label(hasDraft: boolean): string;
  // M4 (#533): the pane shell contract's decided half, the cross-pane
  // sort, the zone bridge, and the waste pane.
  waste_zone_queries_json(inputsJson: string): string;
  waste_facts_json(inputsJson: string, zoneFactsJson: string): string;
  waste_answer_json(inputsJson: string, zoneFactsJson: string): string;
  waste_setup_json(inputsJson: string): string;
  parse_waste_body_json(snapshotJson: string): string;
  pane_zone_queries_json(inputsJson: string, surface: string): string;
  rank_panes_json(inputsJson: string, zoneFactsJson: string, surface: string): string;
  order_panes_json(panesJson: string, questionOrderJson: string): string;
  same_pane_identity_json(aJson: string, bJson: string): boolean;
  pane_band_order_json(): string;
  pane_question_order_json(): string;
  waste_constants_json(): string;
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

/** The three fields `hummingbird_core::decisions::panes::inputs::PaneInputs`
 * reads out of the shell's whole `QuestionInputs` — the same "do not
 * re-cross whole DTOs" discipline as `frontierPayload` above. The sync
 * snapshot, the calendar arms and the item list are read by no sunk rule
 * and never leave the main thread. */
export interface PaneInputsSource {
  nowMs: number;
  bindings: BindingDTO[] | null;
  paneReads: Record<string, PaneReadDTO | undefined>;
}

function paneInputsPayload(inputs: PaneInputsSource): string {
  return JSON.stringify({
    nowMs: inputs.nowMs,
    bindings: inputs.bindings,
    paneReads: inputs.paneReads,
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

/** `hummingbird_core::decisions::panes::rank_panes` — phase two for a whole
 * surface, already in display order, covering only the questions the core
 * decides today (`SUNK`). Exercised by tests; whether the web should hoist
 * its per-question ranking onto this is #533's stated ergonomics question,
 * answered in that PR rather than here. */
export function rankPanesFromCore(
  inputs: PaneInputsSource,
  facts: ZoneFacts,
  surface: PaneSurface,
): RankedPaneLike[] {
  return JSON.parse(
    required().rank_panes_json(paneInputsPayload(inputs), JSON.stringify(facts), surface),
  ) as RankedPaneLike[];
}

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
