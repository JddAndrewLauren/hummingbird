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
export interface DecisionsModule {
  can_submit_capture(draft: string): boolean;
  decisions_probe_item_payload(itemsJson: string): string;
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
  // M1-4 (#502): the item stage-transition rules.
  item_available_actions(stage: string): string;
  item_applied_stage(action: string): string | undefined;
  item_can_mark_done(stage: string, archived: boolean): boolean;
  item_can_grill(stage: string): boolean;
  item_grill_button_label(hasDraft: boolean): string;
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

/** What M1-1's structured-payload benchmark measures — see `decisions.rs`.
 * A probe instrument, not a product read: M1-3 replaces it with the real
 * ordering call. */
export function probeItemPayload(itemsJson: string): string {
  return required().decisions_probe_item_payload(itemsJson);
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
